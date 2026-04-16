// Main test loop for the native backend.
//
// Implements the PbtkitState equivalent: random generation, shrinking,
// and final replay of failing examples.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Once;

use rand::RngExt;
use rand::SeedableRng;
use rand::rngs::SmallRng;

use crate::antithesis::TestLocation;
use crate::control::with_test_context;
use crate::native::core::{
    ChoiceNode, ChoiceValue, NativeTestCase, Span, Status, sort_key,
};
use crate::native::database::NativeDatabase;
use crate::native::shrinker::Shrinker;
use crate::runner::{Database, Settings, Verbosity};
use crate::test_case::{ASSUME_FAIL_STRING, STOP_TEST_STRING, TestCase};

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
}

fn take_panic_info() -> Option<(String, String, String, Backtrace)> {
    LAST_PANIC_INFO.with(|info| info.borrow_mut().take())
}

fn panic_message(payload: &Box<dyn std::any::Any + Send>) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        s.to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "Unknown panic".to_string()
    }
}

const RANDOM_GENERATION_BATCH: u64 = 10;
const SPAN_MUTATION_ATTEMPTS: usize = 5;

/// Entry point for native-backend test execution.
///
/// Called from `Hegel::run()` when the `native` feature is enabled.
pub fn native_run<F>(
    mut test_fn: F,
    settings: &Settings,
    database_key: Option<&str>,
    _test_location: Option<&TestLocation>,
) where
    F: FnMut(TestCase),
{
    init_native_panic_hook();

    let mut rng = create_rng(settings, database_key);
    let max_examples = settings.test_cases;
    let verbosity = settings.verbosity;

    // Build database handle if configured.
    let db: Option<NativeDatabase> = match &settings.database {
        Database::Path(p) => Some(NativeDatabase::new(p)),
        _ => None,
    };

    let mut result: Option<Vec<ChoiceNode>> = None;
    let mut valid_test_cases: u64 = 0;
    let mut calls: u64 = 0;
    let mut test_is_trivial = false;

    // --- Database replay phase ---
    // If a stored counterexample exists for this key, try it before random
    // generation. If it still fails, use it as the starting point for
    // shrinking (which often means shrinking completes immediately because
    // the stored value is already minimal).
    if let (Some(db_ref), Some(key)) = (&db, database_key) {
        if let Some(stored_choices) = db_ref.load(key) {
            let ntc = NativeTestCase::for_choices(&stored_choices, None);
            let (status, nodes, _, _) = run_one_test_case_full(ntc, &mut test_fn, false);
            if status == Status::Interesting {
                result = Some(nodes);
            }
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
            let (status, nodes, spans, _) = run_one_test_case_full(ntc, &mut test_fn, false);
            calls += 1;

            if nodes.is_empty() && status >= Status::Invalid {
                test_is_trivial = true;
            }
            if status >= Status::Valid {
                valid_test_cases += 1;
            }
            if status == Status::Interesting {
                if result.is_none() || sort_key(&nodes) < sort_key(result.as_ref().unwrap()) {
                    result = Some(nodes);
                }
            } else if status == Status::Valid {
                // Try span mutations on this valid test case to find interesting ones.
                let mutation_result = try_span_mutation(&nodes, &spans, &mut rng, &mut test_fn);
                calls += SPAN_MUTATION_ATTEMPTS as u64;
                if let Some(mut_nodes) = mutation_result {
                    if result.is_none() || sort_key(&mut_nodes) < sort_key(result.as_ref().unwrap()) {
                        result = Some(mut_nodes);
                    }
                }
            }
        }
    }

    // --- Shrinking phase ---
    if let Some(ref mut best_nodes) = result {
        if verbosity == Verbosity::Debug {
            eprintln!(
                "Shrinking: initial choice sequence length = {}",
                best_nodes.len()
            );
        }

        // Verify the result is still interesting.
        let choices: Vec<ChoiceValue> = best_nodes.iter().map(|n| n.value.clone()).collect();
        let verify_ntc =
            NativeTestCase::for_choices(&choices, Some(best_nodes));
        let (verify_status, verify_nodes) =
            run_one_test_case(verify_ntc, &mut test_fn, false);
        assert_eq!(
            verify_status,
            Status::Interesting,
            "Result was not reproducibly interesting"
        );
        *best_nodes = verify_nodes;

        let mut shrinker = Shrinker::new(
            Box::new(|candidate_nodes: &[ChoiceNode]| {
                let choices: Vec<ChoiceValue> =
                    candidate_nodes.iter().map(|n| n.value.clone()).collect();
                let ntc =
                    NativeTestCase::for_choices(&choices, Some(candidate_nodes));
                let (status, new_nodes) = run_one_test_case(ntc, &mut test_fn, false);
                calls += 1;

                let is_interesting = status == Status::Interesting;
                (is_interesting, new_nodes.len())
            }),
            best_nodes.clone(),
        );
        shrinker.shrink();
        *best_nodes = shrinker.current_nodes;

        if verbosity == Verbosity::Debug {
            eprintln!(
                "Shrinking complete: final choice sequence length = {}",
                best_nodes.len()
            );
        }
    }

    // --- Save to database ---
    // Persist the shrunk counterexample so subsequent runs can replay it
    // immediately without repeating generation + shrinking.
    if let (Some(db_ref), Some(key), Some(best_nodes)) = (&db, database_key, &result) {
        let choices: Vec<ChoiceValue> = best_nodes.iter().map(|n| n.value.clone()).collect();
        db_ref.save(key, &choices);
    }

    // --- Result handling ---
    // If no valid test cases were found, all examples were filtered by assume().
    // This corresponds to the server's filter_too_much health check situation.
    // When health checks are suppressed, the server silently passes; we do the same.

    if let Some(ref best_nodes) = result {
        // Final replay with output enabled: prints draw labels and panic info.
        let choices: Vec<ChoiceValue> = best_nodes.iter().map(|n| n.value.clone()).collect();
        let ntc = NativeTestCase::for_choices(&choices, Some(best_nodes));
        let (_, _, _, panic_msg) = run_one_test_case_full(ntc, &mut test_fn, true);

        let msg = panic_msg.unwrap_or_else(|| "unknown".to_string());
        panic!("Property test failed: {}", msg);
    }
}

/// Run a single test case and return (status, recorded nodes).
fn run_one_test_case<F: FnMut(TestCase)>(
    ntc: NativeTestCase,
    test_fn: &mut F,
    is_final: bool,
) -> (Status, Vec<ChoiceNode>) {
    let (status, nodes, _, _) = run_one_test_case_full(ntc, test_fn, is_final);
    (status, nodes)
}

/// Run a single test case, returning (status, nodes, spans, optional panic message).
fn run_one_test_case_full<F: FnMut(TestCase)>(
    ntc: NativeTestCase,
    test_fn: &mut F,
    is_final: bool,
) -> (Status, Vec<ChoiceNode>, Vec<Span>, Option<String>) {
    let tc = TestCase::new_native(ntc, is_final);
    let result = with_test_context(|| catch_unwind(AssertUnwindSafe(|| test_fn(tc.clone()))));

    let (status, panic_msg) = match result {
        Ok(()) => (Status::Valid, None),
        Err(e) => {
            let msg = panic_message(&e);
            if msg == ASSUME_FAIL_STRING || msg == STOP_TEST_STRING {
                (Status::Invalid, None)
            } else {
                if is_final {
                    // Print the panic details for the final replay, manually
                    // so there is no blank-line separator (which Rust's default
                    // panic handler adds before "thread '...' panicked").
                    if let Some((thread_name, thread_id, location, backtrace)) = take_panic_info() {
                        eprintln!(
                            "thread '{}' ({}) panicked at {}:",
                            thread_name, thread_id, location
                        );
                        eprintln!("{}", msg);

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
                }
                (Status::Interesting, Some(msg))
            }
        }
    };

    let nodes = tc.take_native_nodes();
    let spans = tc.take_native_spans();
    (status, nodes, spans, panic_msg)
}

/// Format a backtrace captured from inside the panic hook, optionally filtering
/// to the "short" format used by Rust's default panic handler.
///
/// Short format shows only the frames between `__rust_end_short_backtrace` and
/// `__rust_begin_short_backtrace` markers (the user-visible range), then
/// renumbers them starting from 0.  This produces the same frame layout as
/// Rust's own short backtrace, ensuring frame 2 is the user's test closure.
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

/// Try span mutation: find two spans with the same label and replace both with
/// identical choices from one donor. This makes two independently-generated
/// structures (like two strings in a tuple) identical, which is how
/// `test_long_duplicates_strings`-style tests are found.
///
/// Port of pbtkit's `span_mutation.py`.
fn try_span_mutation<F: FnMut(TestCase)>(
    nodes: &[ChoiceNode],
    spans: &[Span],
    rng: &mut SmallRng,
    test_fn: &mut F,
) -> Option<Vec<ChoiceNode>> {
    use std::collections::HashMap;

    // Group span indices by label.
    let mut by_label: HashMap<&str, Vec<usize>> = HashMap::new();
    for (i, span) in spans.iter().enumerate() {
        by_label.entry(span.label.as_str()).or_default().push(i);
    }
    // Only keep labels that have at least 2 spans (needed to make two equal).
    let multi: Vec<Vec<usize>> = by_label.into_values()
        .filter(|v| v.len() >= 2)
        .collect();
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
        let (status, new_nodes, _, _) = run_one_test_case_full(ntc, test_fn, false);

        if status == Status::Interesting {
            return Some(new_nodes);
        }
    }

    None
}

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
