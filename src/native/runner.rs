// Main test loop for the native backend.
//
// Implements the PbtkitState equivalent: random generation, shrinking,
// and final replay of failing examples.

use std::sync::Once;

use rand::RngExt;
use rand::SeedableRng;
use rand::rngs::SmallRng;

use crate::antithesis::TestLocation;
use crate::native::core::{ChoiceNode, ChoiceValue, NativeTestCase, Span, Status, sort_key};
use crate::native::database::{
    ExampleDatabase, NativeDatabase, deserialize_choices, serialize_choices,
};
use crate::native::shrinker::Shrinker;
use crate::native::tree::CachedTestFunction;
use crate::runner::{Database, HealthCheck, Settings, Verbosity};
use crate::test_case::TestCase;

static NATIVE_PANIC_HOOK_INIT: Once = Once::new();

/// Initialise the panic hook (once per process).
///
/// Suppresses panic output while a test case is running (i.e. while in test
/// context). Panics during generation and shrinking are caught by catch_unwind
/// and must not print to stderr. Instead, the panic info (thread name, location,
/// and backtrace) is stored in a thread-local for the final replay to print
/// manually. Manual printing avoids the blank-line separator that Rust's default
/// handler inserts before each "thread 'main' panicked" message.
fn init_native_panic_hook() {
    use crate::control::currently_in_test_context;
    use std::backtrace::Backtrace;

    NATIVE_PANIC_HOOK_INIT.call_once(|| {
        let prev_hook = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            if !currently_in_test_context() {
                prev_hook(info);
                return;
            }

            let thread = std::thread::current();
            let thread_name = thread.name().unwrap_or("<unnamed>").to_string();
            let thread_id = format!("{:?}", thread.id())
                .trim_start_matches("ThreadId(")
                .trim_end_matches(')')
                .to_string();
            let location = info
                .location()
                .map(|loc| format!("{}:{}:{}", loc.file(), loc.line(), loc.column()))
                .unwrap_or_else(|| "<unknown>".to_string());

            let backtrace = Backtrace::capture();

            LAST_PANIC_INFO
                .with(|l| *l.borrow_mut() = Some((thread_name, thread_id, location, backtrace)));
        }));
    });
}

use std::backtrace::Backtrace;
use std::cell::RefCell;

thread_local! {
    static LAST_PANIC_INFO: RefCell<Option<(String, String, String, Backtrace)>> = const { RefCell::new(None) };
    /// Payload from the most recent interesting panic captured during the final
    /// replay.  Used by `native_run` to re-raise the original panic via
    /// `resume_unwind` (which avoids producing a second panic message on stderr).
    static LAST_PANIC_PAYLOAD: RefCell<Option<Box<dyn std::any::Any + Send>>> = const { RefCell::new(None) };
}

fn take_panic_payload() -> Option<Box<dyn std::any::Any + Send>> {
    LAST_PANIC_PAYLOAD.with(|p| p.borrow_mut().take())
}

/// Extract a string message from a panic payload.
// nocov start
pub(crate) fn panic_message(payload: &Box<dyn std::any::Any + Send>) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        s.to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "Unknown panic".to_string()
    }
}
// nocov end

/// Print the panic details for the final replay, then store the wrapped
/// payload for re-raising via `resume_unwind`.
///
/// Called from `CachedTestFunction::execute` when `is_final` is true.
pub(crate) fn store_final_panic_info(msg: &str) {
    if let Some((thread_name, thread_id, location, backtrace)) =
        LAST_PANIC_INFO.with(|l| l.borrow_mut().take())
    {
        eprintln!(
            "thread '{}' ({}) panicked at {}:",
            thread_name, thread_id, location
        );
        eprintln!("Property test failed: {}", msg);

        if backtrace.status() == std::backtrace::BacktraceStatus::Captured {
            let is_full = std::env::var("RUST_BACKTRACE")
                .map(|v| v == "full")
                .unwrap_or(false);
            let formatted = format_backtrace_native(&backtrace, is_full);
            eprintln!("stack backtrace:\n{}", formatted);
            if !is_full {
                eprintln!(
                    "note: Some details are omitted, run with `RUST_BACKTRACE=full` for a verbose backtrace."
                );
            }
        }
    }
    // Wrap the message to match the server backend's format
    // ("Property test failed: <original>"), then store it so
    // native_run can re-raise via resume_unwind.  Wrapping lets
    // existing test helpers that check for "Property test
    // failed" work identically in both backends.  Using
    // resume_unwind avoids calling the panic hook a second time
    // (no duplicate stderr message).
    let wrapped: Box<dyn std::any::Any + Send> = Box::new(format!("Property test failed: {msg}"));
    LAST_PANIC_PAYLOAD.with(|p| *p.borrow_mut() = Some(wrapped));
}

const RANDOM_GENERATION_BATCH: u64 = 10;
const SPAN_MUTATION_ATTEMPTS: usize = 5;

/// Maximum number of consecutive filtered (assume()-failed) test cases before
/// FilterTooMuch is reported.  Mirrors Hypothesis's `max_invalid_draws` setting,
/// but scaled up slightly to be less sensitive to mild filtering.
const FILTER_TOO_MUCH_THRESHOLD: u64 = 200;

/// Cumulative wall-clock threshold across the generation phase before
/// TooSlow fires. Hypothesis's `engine.py` checks `total_draw_time` against
/// `max(1.0s, 5 * deadline)`; with the default 200 ms deadline this floors
/// at 1 second, which is what we mirror here. Hypothesis tracks draw time
/// only, but until the native engine separates draw time from test
/// execution we approximate it with whole-test wall-clock.
const TOO_SLOW_THRESHOLD: std::time::Duration = std::time::Duration::from_secs(1);

/// Health checks (TooSlow / FilterTooMuch) are evaluated only while the run
/// has fewer than this many valid examples on record. Mirrors Hypothesis's
/// `max_valid_draws = 10` in `record_for_health_check`: once the first ten
/// valid examples have been observed, the health-check window closes and
/// later slow or filtered draws no longer trigger a failure.
const HEALTH_CHECK_MAX_VALID: u64 = 10;

/// Entry point for native-backend test execution.
///
/// Called from `Hegel::run()` when the `native` feature is enabled.
// nocov start
pub fn native_run<F>(
    test_fn: F,
    settings: &Settings,
    database_key: Option<&str>,
    test_location: Option<&TestLocation>,
) where
    F: FnMut(TestCase),
{
    init_native_panic_hook();

    let mut rng = create_rng(settings, database_key);
    let max_examples = settings.test_cases;
    let verbosity = settings.verbosity;

    // Build database handle if configured.
    let db: Option<Box<dyn ExampleDatabase>> = match &settings.database {
        Database::Path(p) => Some(Box::new(NativeDatabase::new(p))),
        _ => None,
    };

    // The CachedTestFunction wraps the user's test function. All test
    // execution goes through it, which ensures every run is recorded in
    // the data tree (for non-determinism detection) and, during shrinking,
    // checked against the result cache.
    let mut ctf = CachedTestFunction::new(test_fn);

    let mut result: Option<Vec<ChoiceNode>> = None;
    let mut valid_test_cases: u64 = 0;
    let mut calls: u64 = 0;
    let mut test_is_trivial = false;
    let mut invalid_calls: u64 = 0;
    let mut total_test_time = std::time::Duration::ZERO;
    // True when `result` came from a database replay that matched the
    // stored choice sequence exactly (same length = same test shape). In
    // that case we trust the stored value is already shrunk and skip the
    // shrinking phase entirely. If the replay misaligns (fewer nodes
    // consumed than stored), the test shape has changed and we re-shrink.
    let mut replay_aligned = false;

    // --- Database replay phase ---
    // Fetch every stored value for this key, sort shortlex (shortest
    // first, then lex on bytes — see `shortlex` in
    // `conjecture/engine.py`), and try them in order. The first
    // still-interesting value is reused as the starting point for
    // shrinking; values that are corrupt or no longer interesting are
    // evicted from the database. Mirrors Hypothesis's
    // `reuse_existing_examples`.
    if let (Some(db_ref), Some(key)) = (&db, database_key) {
        let key_bytes = key.as_bytes();
        let mut values = db_ref.fetch(key_bytes);
        values.sort_by(|a, b| a.len().cmp(&b.len()).then_with(|| a.cmp(b)));
        for raw in values {
            let Some(stored_choices) = deserialize_choices(&raw) else {
                db_ref.delete(key_bytes, &raw);
                continue;
            };
            let ntc = NativeTestCase::for_choices(&stored_choices, None);
            let (status, nodes, _) = ctf.run(ntc);
            if status == Status::Interesting {
                replay_aligned = nodes.len() == stored_choices.len();
                result = Some(nodes);
                break;
            }
            db_ref.delete(key_bytes, &raw);
        }
    }

    // --- Generation phase ---
    while !test_is_trivial
        && result.is_none()
        && valid_test_cases < max_examples
        && calls < max_examples * 10
    {
        // Run a batch of random test cases (like pbtkit's random_generation).
        for _ in 0..RANDOM_GENERATION_BATCH {
            if test_is_trivial
                || result.is_some()
                || valid_test_cases >= max_examples
                || calls >= max_examples * 10
            {
                break;
            }

            let batch_rng = SmallRng::from_rng(&mut rng);
            let ntc = NativeTestCase::new_random(batch_rng);
            if verbosity == Verbosity::Verbose {
                eprintln!("Trying example: ");
            }
            let tc_start = std::time::Instant::now();
            let (status, nodes, spans) = ctf.run(ntc);
            total_test_time += tc_start.elapsed();
            calls += 1;
            if verbosity == Verbosity::Debug {
                eprintln!(
                    "test case #{calls}: status = {status:?}, choices = {}",
                    nodes.len()
                );
            }

            // TooSlow health check: if cumulative test execution time exceeds
            // the threshold, report it. Mirrors Hypothesis's `total_draw_time`
            // check in `conjecture/engine.py`. The check is only active while
            // we are still in the first `HEALTH_CHECK_MAX_VALID` valid examples;
            // once that window closes, later slow examples are tolerated.
            if valid_test_cases < HEALTH_CHECK_MAX_VALID
                && total_test_time > TOO_SLOW_THRESHOLD
                && !settings
                    .suppress_health_check
                    .contains(&HealthCheck::TooSlow)
            {
                panic!(
                    "FailedHealthCheck: TooSlow — input generation is slow: \
                     only {valid_test_cases} valid inputs after {:?} (threshold \
                     {:?}). Slow generation makes property testing much less \
                     effective. If this is expected, suppress the check with \
                     suppress_health_check = [HealthCheck::TooSlow].",
                    total_test_time, TOO_SLOW_THRESHOLD
                );
            }

            if nodes.is_empty() && status >= Status::Invalid {
                test_is_trivial = true;
            }
            if status >= Status::Valid {
                valid_test_cases += 1;
            }
            if status == Status::Invalid {
                invalid_calls += 1;
                // FilterTooMuch health check: if a large number of consecutive test
                // cases are all filtered out (via assume()) before any valid example
                // is found, report a health check failure.
                if invalid_calls >= FILTER_TOO_MUCH_THRESHOLD
                    && valid_test_cases == 0
                    && !settings
                        .suppress_health_check
                        .contains(&HealthCheck::FilterTooMuch)
                {
                    panic!(
                        "FailedHealthCheck: FilterTooMuch — it looks like this \
                         test is filtering out too many inputs. \
                         {invalid_calls} inputs were filtered out by assume() \
                         before any valid input was generated. \
                         If this is expected, suppress the check with \
                         suppress_health_check = [HealthCheck::FilterTooMuch]."
                    );
                }
            } else {
                invalid_calls = 0;
            }
            if status == Status::Interesting {
                if result.is_none() || sort_key(&nodes) < sort_key(result.as_ref().unwrap()) {
                    result = Some(nodes);
                }
            } else if status == Status::Valid {
                // Try span mutations on this valid test case to find interesting ones.
                let mutation_result = try_span_mutation(&nodes, &spans, &mut rng, &mut ctf);
                calls += SPAN_MUTATION_ATTEMPTS as u64;
                if let Some(mut_nodes) = mutation_result {
                    if result.is_none() || sort_key(&mut_nodes) < sort_key(result.as_ref().unwrap())
                    {
                        result = Some(mut_nodes);
                    }
                }
            }
        }
    }

    // --- Shrinking phase ---
    if let Some(ref mut best_nodes) = result {
        if replay_aligned {
            if verbosity == Verbosity::Debug {
                eprintln!(
                    "Skipping shrink: reused aligned database replay of length {}",
                    best_nodes.len()
                );
            }
        } else {
            if verbosity == Verbosity::Debug {
                eprintln!(
                    "Shrinking: initial choice sequence length = {}",
                    best_nodes.len()
                );
            }

            // Verify the result is still interesting.
            let choices: Vec<ChoiceValue> = best_nodes.iter().map(|n| n.value.clone()).collect();
            let verify_ntc = NativeTestCase::for_choices(&choices, Some(best_nodes));
            let (verify_status, verify_nodes, _) = ctf.run(verify_ntc);
            assert_eq!(
                verify_status,
                Status::Interesting,
                "Result was not reproducibly interesting"
            );
            *best_nodes = verify_nodes;

            {
                let mut shrinker = Shrinker::new(
                    Box::new(|candidate_nodes: &[ChoiceNode]| {
                        if verbosity == Verbosity::Verbose {
                            eprintln!("Trying example: ");
                        }
                        let result = ctf.run_shrink(candidate_nodes);
                        calls += 1;
                        result
                    }),
                    best_nodes.clone(),
                );
                shrinker.shrink();
                *best_nodes = shrinker.current_nodes;
            }

            if verbosity == Verbosity::Debug {
                eprintln!(
                    "Shrinking complete: final choice sequence length = {}",
                    best_nodes.len()
                );
            }
        }
    }

    // --- Save to database ---
    // Persist the shrunk counterexample so subsequent runs can replay it
    // immediately without repeating generation + shrinking. Multiple
    // distinct shrunk counterexamples may accumulate under the same
    // key across runs (e.g. if the test was altered); the replay loop
    // above evicts values that no longer interest the test.
    if let (Some(db_ref), Some(key), Some(best_nodes)) = (&db, database_key, &result) {
        let choices: Vec<ChoiceValue> = best_nodes.iter().map(|n| n.value.clone()).collect();
        db_ref.save(key.as_bytes(), &serialize_choices(&choices));
    }

    // --- Antithesis integration ---
    // Mirror the server backend: if ANTITHESIS_OUTPUT_DIR is set, either panic
    // (no antithesis feature) or emit declaration + evaluation to sdk.jsonl.
    let test_failed = result.is_some();
    use crate::antithesis::is_running_in_antithesis;
    crate::antithesis::require_antithesis_feature(
        is_running_in_antithesis(),
        cfg!(feature = "antithesis"),
    );

    #[cfg(feature = "antithesis")]
    if is_running_in_antithesis() {
        if let Some(loc) = test_location {
            crate::antithesis::emit_assertion(loc, !test_failed);
        }
    }
    // Suppress unused-variable warnings for the non-antithesis-feature build: both
    // variables are only consumed inside the cfg(feature = "antithesis") block above.
    let _ = (test_location, test_failed);

    // --- Result handling ---
    // If no valid test cases were found, all examples were filtered by assume().
    // This corresponds to the server's filter_too_much health check situation.
    // When health checks are suppressed, the server silently passes; we do the same.

    if let Some(ref best_nodes) = result {
        // Final replay with output enabled: prints draw labels and panic info.
        let choices: Vec<ChoiceValue> = best_nodes.iter().map(|n| n.value.clone()).collect();
        let ntc = NativeTestCase::for_choices(&choices, Some(best_nodes));
        let (status, _, _) = ctf.run_final(ntc);

        if status == Status::Interesting {
            // Re-raise the original panic payload.  resume_unwind bypasses the
            // panic hook so there is no second "thread '...' panicked" line on
            // stderr — only the manually-printed output from the is_final block
            // above is visible.
            if let Some(payload) = take_panic_payload() {
                std::panic::resume_unwind(payload);
            }
            // This branch should be unreachable: if the final replay is
            // Interesting, the panic hook must have stored a payload.
            panic!(
                "BUG: final replay was Interesting but no panic payload was stored; this is a bug in the native runner"
            );
        } else {
            // The replay passed even though we had a shrunk counterexample.
            // This means the test outcome depends on external state — it is
            // flaky.
            panic!(
                "Flaky test detected: Your test produced different outcomes \
                 when run with the same generated data — it failed when it \
                 previously succeeded, or succeeded when it previously failed. \
                 This usually means your test depends on external state such as \
                 global variables, system time, or external random number generators."
            );
        }
    }
}
// nocov end

/// Format a backtrace captured from inside the panic hook, optionally filtering
/// to the "short" format used by Rust's default panic handler.
///
/// Short format shows only the frames between `__rust_end_short_backtrace` and
/// `__rust_begin_short_backtrace` markers (the user-visible range), then
/// renumbers them starting from 0.  This produces the same frame layout as
/// Rust's own short backtrace, ensuring frame 2 is the user's test closure.
// nocov start
fn format_backtrace_native(bt: &Backtrace, full: bool) -> String {
    let backtrace_str = format!("{}", bt);

    if full {
        return backtrace_str;
    }

    let lines: Vec<&str> = backtrace_str.lines().collect();
    let mut start_idx = 0;
    let mut end_idx = lines.len();

    for (i, line) in lines.iter().enumerate() {
        if line.contains("__rust_end_short_backtrace") {
            for (j, next_line) in lines.iter().enumerate().skip(i + 1) {
                if next_line
                    .trim_start()
                    .chars()
                    .next()
                    .map(|c| c.is_ascii_digit())
                    .unwrap_or(false)
                {
                    start_idx = j;
                    break;
                }
            }
        }
        if line.contains("__rust_begin_short_backtrace") {
            for (j, prev_line) in lines
                .iter()
                .enumerate()
                .take(i + 1)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
            {
                if prev_line
                    .trim_start()
                    .chars()
                    .next()
                    .map(|c| c.is_ascii_digit())
                    .unwrap_or(false)
                {
                    end_idx = j;
                    break;
                }
            }
            break;
        }
    }

    let filtered: Vec<&str> = lines[start_idx..end_idx].to_vec();
    let mut new_frame_num = 0usize;
    let mut result = Vec::new();
    for line in filtered {
        let trimmed = line.trim_start();
        if trimmed
            .chars()
            .next()
            .map(|c| c.is_ascii_digit())
            .unwrap_or(false)
        {
            if let Some(colon_pos) = trimmed.find(':') {
                let rest = &trimmed[colon_pos..];
                result.push(format!("{:>4}{}", new_frame_num, rest));
                new_frame_num += 1;
            } else {
                result.push(line.to_string());
            }
        } else {
            result.push(line.to_string());
        }
    }
    result.join("\n")
}
// nocov end

/// Try span mutation: find two spans with the same label and replace both with
/// identical choices from one donor. This makes two independently-generated
/// structures (like two strings in a tuple) identical, which is how
/// `test_long_duplicates_strings`-style tests are found.
///
/// Port of pbtkit's `span_mutation.py`.
// nocov start
fn try_span_mutation<F: FnMut(TestCase)>(
    nodes: &[ChoiceNode],
    spans: &[Span],
    rng: &mut SmallRng,
    ctf: &mut CachedTestFunction<F>,
) -> Option<Vec<ChoiceNode>> {
    use std::collections::HashMap;

    // Group span indices by label.
    let mut by_label: HashMap<&str, Vec<usize>> = HashMap::new();
    for (i, span) in spans.iter().enumerate() {
        by_label.entry(span.label.as_str()).or_default().push(i);
    }
    // Only keep labels that have at least 2 spans (needed to make two equal).
    let multi: Vec<Vec<usize>> = by_label.into_values().filter(|v| v.len() >= 2).collect();
    if multi.is_empty() {
        return None;
    }

    let values: Vec<ChoiceValue> = nodes.iter().map(|n| n.value.clone()).collect();

    for _ in 0..SPAN_MUTATION_ATTEMPTS {
        let group = &multi[rng.random_range(0..multi.len())];

        // Pick two distinct span indices from this group.
        let i_a = rng.random_range(0..group.len());
        let mut i_b = rng.random_range(0..group.len() - 1);
        if i_b >= i_a {
            i_b += 1;
        }

        let mut span_a = &spans[group[i_a]];
        let mut span_b = &spans[group[i_b]];
        // Ensure span_a comes before span_b in the choice sequence.
        if span_a.start > span_b.start {
            std::mem::swap(&mut span_a, &mut span_b);
        }
        // Skip overlapping spans.
        if span_a.end > span_b.start {
            continue;
        }

        // Pick one of a/b as donor; replace both with donor's choices.
        let donor = if rng.random::<bool>() { span_a } else { span_b };
        let replacement: Vec<ChoiceValue> = values[donor.start..donor.end].to_vec();

        // Build the mutated choice sequence:
        // values[:a.start] + replacement + values[a.end..b.start] + replacement + values[b.end..]
        let mut attempt: Vec<ChoiceValue> = Vec::new();
        attempt.extend_from_slice(&values[..span_a.start]);
        attempt.extend(replacement.iter().cloned());
        attempt.extend_from_slice(&values[span_a.end..span_b.start]);
        attempt.extend(replacement.iter().cloned());
        attempt.extend_from_slice(&values[span_b.end..]);

        let ntc = NativeTestCase::for_choices(&attempt, None);
        let (status, new_nodes, _) = ctf.run(ntc);

        if status == Status::Interesting {
            return Some(new_nodes);
        }
    }

    None
}
// nocov end

fn create_rng(settings: &Settings, database_key: Option<&str>) -> SmallRng {
    if let Some(seed) = settings.seed {
        SmallRng::seed_from_u64(seed)
    } else if settings.derandomize {
        let key = database_key.unwrap_or("unnamed-test");
        let hash = hash_string(key);
        SmallRng::seed_from_u64(hash)
    } else {
        SmallRng::from_rng(&mut rand::rng())
    }
}

/// Simple string hashing for derandomize mode.
fn hash_string(s: &str) -> u64 {
    // FNV-1a hash
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in s.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

#[cfg(test)]
#[path = "../../tests/embedded/native/runner_tests.rs"]
mod tests;
