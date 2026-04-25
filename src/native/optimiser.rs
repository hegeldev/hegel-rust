// Targeted property-based testing: port of Hypothesis's
// `conjecture/optimiser.py` and the `target_observations` /
// `best_observed_targets` plumbing on `ConjectureRunner`.
//
// This is a minimal, standalone implementation designed specifically for the
// ported `tests/hypothesis/conjecture_optimiser.rs`. It reuses
// `NativeTestCase` for per-call choice bookkeeping but owns its own
// generation/hill-climbing loop rather than weaving into `native_run`; the
// target-observation hooks are not required for plain test runs, and
// bolting them onto the production runner would be a far larger change than
// the ported tests need.
//
// Hypothesis references:
//   - `internal/conjecture/optimiser.py::Optimiser.hill_climb`
//   - `internal/conjecture/engine.py::ConjectureRunner.optimise_targets`
//   - `internal/conjecture/junkdrawer.py::find_integer`

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::panic::{AssertUnwindSafe, catch_unwind};

use rand::rngs::SmallRng;
use rand::{RngExt, SeedableRng};

use crate::native::core::{
    BUFFER_SIZE, ChoiceKind, ChoiceNode, ChoiceValue, NativeTestCase, Span, Status,
};
use crate::native::intervalsets::IntervalSet;

thread_local! {
    /// Per-thread override of [`BUFFER_SIZE`]. Set by [`BufferSizeLimit`] for
    /// the duration of its lifetime; read by [`TargetedRunner`] when
    /// constructing probing `NativeTestCase`s.
    static BUFFER_SIZE_OVERRIDE: RefCell<Option<usize>> = const { RefCell::new(None) };
}

fn current_buffer_size() -> usize {
    BUFFER_SIZE_OVERRIDE.with(|c| c.borrow().unwrap_or(BUFFER_SIZE))
}

/// Panic payload raised by [`TargetedTestCase::mark_invalid`] to unwind out of
/// the user's test body. Caught inside [`TargetedRunner::run_on`].
const MARK_INVALID_PANIC: &str = "__hegel_targeted_mark_invalid__";
/// Panic payload raised when an underlying `NativeTestCase` draw returns
/// `StopTest` (buffer exhausted). Treated as `Status::EarlyStop`.
const STOP_TEST_PANIC: &str = "__hegel_targeted_stop_test__";

/// Sentinel returned from [`TargetedRunner::optimise_targets`] when the
/// `max_examples` budget is exhausted mid-climb. Port of Hypothesis's
/// `RunIsComplete`.
#[derive(Debug, Clone, Copy)]
pub struct RunIsComplete;

/// Settings snapshot for [`TargetedRunner`]. Only `max_examples` is tunable;
/// everything else matches Hypothesis defaults.
#[derive(Clone)]
pub struct TargetedRunnerSettings {
    pub max_examples: usize,
}

impl TargetedRunnerSettings {
    pub fn new() -> Self {
        TargetedRunnerSettings { max_examples: 100 }
    }

    pub fn max_examples(mut self, n: usize) -> Self {
        self.max_examples = n;
        self
    }
}

impl Default for TargetedRunnerSettings {
    fn default() -> Self {
        Self::new()
    }
}

/// Result of a single [`TargetedRunner::cached_test_function`] call.
#[non_exhaustive]
pub struct CachedTestResult {
    pub status: Status,
}

/// Test-case surface passed to the runner callback. Exposes a mutable
/// `target_observations` map (the hill-climber's objective) plus the draw,
/// span, and invalidity methods exercised by `test_optimiser.py`.
#[non_exhaustive]
pub struct TargetedTestCase {
    ntc: NativeTestCase,
    pub target_observations: HashMap<String, f64>,
}

impl TargetedTestCase {
    pub fn draw_integer(&mut self, min_value: i128, max_value: i128) -> i128 {
        match self.ntc.draw_integer(min_value, max_value) {
            Ok(v) => v,
            Err(_) => std::panic::panic_any(STOP_TEST_PANIC),
        }
    }

    pub fn draw_boolean(&mut self, p: f64) -> bool {
        match self.ntc.weighted(p, None) {
            Ok(v) => v,
            Err(_) => std::panic::panic_any(STOP_TEST_PANIC),
        }
    }

    pub fn draw_bytes(&mut self, min_size: usize, max_size: usize) -> Vec<u8> {
        match self.ntc.draw_bytes(min_size, max_size) {
            Ok(v) => v,
            Err(_) => std::panic::panic_any(STOP_TEST_PANIC),
        }
    }

    pub fn draw_float(
        &mut self,
        min_value: f64,
        max_value: f64,
        allow_nan: bool,
        allow_infinity: bool,
    ) -> f64 {
        match self
            .ntc
            .draw_float(min_value, max_value, allow_nan, allow_infinity)
        {
            Ok(v) => v,
            Err(_) => std::panic::panic_any(STOP_TEST_PANIC),
        }
    }

    /// Draw a string whose codepoints lie in `intervals`. The current
    /// implementation collapses the interval set to its outer `(min, max)`
    /// bounds — sufficient for the single-range `@example` row ported from
    /// `test_optimising_all_nodes`.
    pub fn draw_string(
        &mut self,
        intervals: &IntervalSet,
        min_size: usize,
        max_size: usize,
    ) -> String {
        let (min_cp, max_cp) = if intervals.intervals.is_empty() {
            (0, 0x10FFFF)
        } else {
            (
                intervals.intervals[0].0,
                intervals.intervals.last().unwrap().1,
            )
        };
        match self.ntc.draw_string(min_cp, max_cp, min_size, max_size) {
            Ok(v) => v,
            Err(_) => std::panic::panic_any(STOP_TEST_PANIC),
        }
    }

    pub fn mark_invalid(&mut self) {
        std::panic::panic_any(MARK_INVALID_PANIC);
    }

    pub fn start_span(&mut self, label: u64) {
        self.ntc.start_span(label);
    }

    pub fn stop_span(&mut self) {
        self.ntc.stop_span(false);
    }
}

type TestFn = Box<dyn FnMut(&mut TargetedTestCase)>;

/// Hashable key derived from a choice sequence; mirrors
/// `ConjectureRunner._cache_key`.
type CacheKey = Vec<u8>;

#[derive(Clone)]
struct CachedRun {
    status: Status,
    nodes: Vec<ChoiceNode>,
    spans: Vec<Span>,
    observations: HashMap<String, f64>,
}

/// Shortlex-style ordering on choice sequences: shorter is simpler; same
/// length tie-breaks lexicographically on per-choice sort keys. Mirrors
/// Hypothesis's `shrinker.sort_key` restricted to the value components.
fn sort_key_less(a: &[ChoiceValue], b: &[ChoiceValue]) -> bool {
    if a.len() != b.len() {
        return a.len() < b.len();
    }
    for (x, y) in a.iter().zip(b) {
        match (x, y) {
            (ChoiceValue::Integer(p), ChoiceValue::Integer(q)) => {
                // Simplest = 0 if 0 is in range, else the bound closest to 0;
                // we approximate with unsigned-magnitude ordering which
                // matches Hypothesis's integer `choice_to_index` well enough
                // for optimiser tie-breaks.
                let pk = (p.unsigned_abs(), *p < 0);
                let qk = (q.unsigned_abs(), *q < 0);
                match pk.cmp(&qk) {
                    std::cmp::Ordering::Less => return true,
                    std::cmp::Ordering::Greater => return false,
                    std::cmp::Ordering::Equal => {}
                }
            }
            (ChoiceValue::Boolean(p), ChoiceValue::Boolean(q)) => match p.cmp(q) {
                std::cmp::Ordering::Less => return true,
                std::cmp::Ordering::Greater => return false,
                std::cmp::Ordering::Equal => {}
            },
            (ChoiceValue::Float(p), ChoiceValue::Float(q)) => match p.total_cmp(q) {
                std::cmp::Ordering::Less => return true,
                std::cmp::Ordering::Greater => return false,
                std::cmp::Ordering::Equal => {}
            },
            (ChoiceValue::Bytes(p), ChoiceValue::Bytes(q)) => match p.cmp(q) {
                std::cmp::Ordering::Less => return true,
                std::cmp::Ordering::Greater => return false,
                std::cmp::Ordering::Equal => {}
            },
            (ChoiceValue::String(p), ChoiceValue::String(q)) => match p.cmp(q) {
                std::cmp::Ordering::Less => return true,
                std::cmp::Ordering::Greater => return false,
                std::cmp::Ordering::Equal => {}
            },
            _ => return false,
        }
    }
    false
}

fn encode_choice_key(choices: &[ChoiceValue]) -> CacheKey {
    // A trivial deterministic encoding: tag byte + variant-specific payload
    // joined with a separator. Uniqueness is what matters, not compactness.
    let mut out = Vec::with_capacity(choices.len() * 8);
    for c in choices {
        match c {
            ChoiceValue::Integer(n) => {
                out.push(0);
                out.extend_from_slice(&n.to_le_bytes());
            }
            ChoiceValue::Boolean(b) => {
                out.push(1);
                out.push(u8::from(*b));
            }
            ChoiceValue::Float(f) => {
                out.push(2);
                out.extend_from_slice(&f.to_bits().to_le_bytes());
            }
            ChoiceValue::Bytes(b) => {
                out.push(3);
                out.extend_from_slice(&(b.len() as u64).to_le_bytes());
                out.extend_from_slice(b);
            }
            ChoiceValue::String(s) => {
                out.push(4);
                out.extend_from_slice(&(s.len() as u64).to_le_bytes());
                for cp in s {
                    out.extend_from_slice(&cp.to_le_bytes());
                }
            }
        }
        out.push(0xff);
    }
    out
}

/// Port of the subset of Hypothesis's `ConjectureRunner` used by
/// `test_optimiser.py` — seeding via `cached_test_function`, hill-climbing
/// via `optimise_targets`, and bookkeeping for `best_observed_targets`.
pub struct TargetedRunner {
    test_fn: TestFn,
    max_examples: usize,
    rng: SmallRng,
    best_observed_targets: HashMap<String, f64>,
    /// Choice sequences that produced the current best score for each target.
    /// Hypothesis stores full `ConjectureResult`s here; we re-run to recover
    /// nodes when needed, trading a few extra calls for a smaller type.
    best_choices_for_target: HashMap<String, Vec<ChoiceValue>>,
    /// Cache of test-run results keyed by the input's "used prefix" — the
    /// first `nodes.len()` choices of the input that produced the cached
    /// result. Mirrors what `ConjectureRunner.__data_cache` +
    /// `DataTree.simulate_test_function` buy upstream: any later input whose
    /// prefix matches a cached key is guaranteed to replay identically up to
    /// the termination point, so its result can be returned without re-running
    /// the test.
    cache: HashMap<CacheKey, CachedRun>,
    /// Count of valid examples that have been produced. Matches upstream's
    /// `valid_examples` counter, which is what gates `max_examples` in
    /// `ConjectureRunner.should_generate_more`. Invalid or EarlyStop runs
    /// are not counted against the budget.
    valid_examples: usize,
}

impl TargetedRunner {
    pub fn new<F>(test_fn: F, settings: TargetedRunnerSettings, rng: SmallRng) -> Self
    where
        F: FnMut(&mut TargetedTestCase) + 'static,
    {
        TargetedRunner {
            test_fn: Box::new(test_fn),
            max_examples: settings.max_examples,
            rng,
            best_observed_targets: HashMap::new(),
            best_choices_for_target: HashMap::new(),
            cache: HashMap::new(),
            valid_examples: 0,
        }
    }

    /// Execute the user test on a prepared [`NativeTestCase`], catching
    /// `mark_invalid` and buffer-exhaustion panics. Updates
    /// `best_observed_targets` in the `status >= Valid` branch.
    fn run_on(
        &mut self,
        ntc: NativeTestCase,
    ) -> (Status, Vec<ChoiceNode>, Vec<Span>, HashMap<String, f64>) {
        let mut tc = TargetedTestCase {
            ntc,
            target_observations: HashMap::new(),
        };

        let test_fn = &mut *self.test_fn;
        let result = catch_unwind(AssertUnwindSafe(|| {
            test_fn(&mut tc);
        }));

        let status = match result {
            Ok(()) => Status::Valid,
            Err(payload) => {
                if let Some(s) = payload.downcast_ref::<&'static str>() {
                    if *s == MARK_INVALID_PANIC {
                        Status::Invalid
                    } else if *s == STOP_TEST_PANIC {
                        Status::EarlyStop
                    } else {
                        std::panic::resume_unwind(payload)
                    }
                } else {
                    std::panic::resume_unwind(payload)
                }
            }
        };

        let nodes = std::mem::take(&mut tc.ntc.nodes);
        let spans = std::mem::take(&mut tc.ntc.spans).into_vec();
        let observations = tc.target_observations;

        if status >= Status::Valid {
            self.valid_examples += 1;
            let choices: Vec<ChoiceValue> = nodes.iter().map(|n| n.value.clone()).collect();
            for (k, v) in &observations {
                let entry = self
                    .best_observed_targets
                    .entry(k.clone())
                    .or_insert(f64::NEG_INFINITY);
                let prev_score = *entry;
                if *v > prev_score {
                    *entry = *v;
                }
                // Update best_choices_for_target if this is a strict
                // improvement OR a lateral move with a smaller sort_key
                // (shortlex ordering). Port of Hypothesis's
                // `best_examples_of_observed_targets` tie-break in
                // `engine.py`.
                let is_improvement = *v > prev_score;
                let is_shortlex_tie = (*v - prev_score).abs() < f64::EPSILON
                    && self
                        .best_choices_for_target
                        .get(k)
                        .is_none_or(|existing| sort_key_less(&choices, existing));
                if is_improvement || is_shortlex_tie {
                    self.best_choices_for_target
                        .insert(k.clone(), choices.clone());
                }
            }
        }

        (status, nodes, spans, observations)
    }

    /// Look for a cached run whose input is a proper prefix of `choices`.
    /// A cache entry is reusable for a longer input iff the cached test run
    /// didn't draw past its own prefix (`cached.nodes.len() <= cached_input_len`),
    /// since in that case the test body would terminate at exactly the same
    /// node count on the longer input.
    ///
    /// `self.cache` only stores entries satisfying that invariant (see the
    /// caching guards in [`run_extend_full`] and [`run_exact`]), so any hit
    /// on a prefix key is safe to return.
    ///
    /// Runs in O(choices.len()) in the worst case, but hits fast for the
    /// common hill-climb pattern where many probes diverge at a shallow
    /// position and terminate quickly.
    fn lookup_prefix_cache(&self, choices: &[ChoiceValue]) -> Option<CachedRun> {
        for l in (1..choices.len()).rev() {
            let key = encode_choice_key(&choices[..l]);
            if let Some(cached) = self.cache.get(&key) {
                return Some(cached.clone());
            }
        }
        None
    }

    pub fn cached_test_function(&mut self, choices: &[ChoiceValue]) -> CachedTestResult {
        let (status, _, _, _) = self.run_exact(choices);
        CachedTestResult { status }
    }

    /// `cached_test_function` with `extend=N` from Hypothesis: after replaying
    /// `choices`, fill up to `extend` additional random draws.
    pub fn cached_test_function_extend(
        &mut self,
        choices: &[ChoiceValue],
        extend: usize,
    ) -> CachedTestResult {
        let seed: u64 = self.rng.random();
        let rng = SmallRng::seed_from_u64(seed);
        let max_size = (choices.len() + extend).min(current_buffer_size());
        let ntc = NativeTestCase::for_probe(choices, rng, max_size);
        let (status, nodes, spans, observations) = self.run_on(ntc);
        // Cache the seed run under the actual choices it generated. This makes
        // later hill-climb probes that happen to replay the same prefix hit
        // the cache instead of paying another test call.
        let drawn: Vec<ChoiceValue> = nodes.iter().map(|n| n.value.clone()).collect();
        self.try_cache(&drawn, &status, &nodes, &spans, &observations);
        CachedTestResult { status }
    }

    /// `cached_test_function` with `extend="full"` — extend up to the current
    /// buffer size. Used internally by the hill-climber. Results from runs
    /// that didn't need to draw past the prefix are cached for reuse;
    /// extensions are nondeterministic and never cached.
    ///
    /// Cache lookup also covers PREFIX matches: if some proper prefix
    /// `choices[..L]` was previously run and produced a deterministic result
    /// (i.e. `nodes.len() <= L`, the test didn't draw past `L`), the same
    /// result applies to any longer input starting with that prefix. This
    /// lets us skip re-running the test in the common hill-climb pattern
    /// where we probe variants that terminate before they reach the modified
    /// choice. Conceptually mirrors what Hypothesis gets "for free" from
    /// `DataTree.simulate_test_function`.
    fn run_extend_full(
        &mut self,
        choices: &[ChoiceValue],
    ) -> (Status, Vec<ChoiceNode>, Vec<Span>, HashMap<String, f64>) {
        let key = encode_choice_key(choices);
        if let Some(cached) = self.cache.get(&key).cloned() {
            return (
                cached.status,
                cached.nodes,
                cached.spans,
                cached.observations,
            );
        }
        if let Some(cached) = self.lookup_prefix_cache(choices) {
            return (
                cached.status,
                cached.nodes,
                cached.spans,
                cached.observations,
            );
        }
        let seed: u64 = self.rng.random();
        let rng = SmallRng::seed_from_u64(seed);
        let max_size = current_buffer_size();
        let ntc = NativeTestCase::for_probe(choices, rng, max_size);
        let (status, nodes, spans, observations) = self.run_on(ntc);
        self.try_cache(choices, &status, &nodes, &spans, &observations);
        (status, nodes, spans, observations)
    }

    /// Like [`run_extend_full`] but using the prefix choices exactly (no
    /// `extend`). Mirrors `cached_test_function(..., extend=0)`. Results are
    /// deterministic (no random tail) and always cached.
    fn run_exact(
        &mut self,
        choices: &[ChoiceValue],
    ) -> (Status, Vec<ChoiceNode>, Vec<Span>, HashMap<String, f64>) {
        let key = encode_choice_key(choices);
        if let Some(cached) = self.cache.get(&key).cloned() {
            return (
                cached.status,
                cached.nodes,
                cached.spans,
                cached.observations,
            );
        }
        if let Some(cached) = self.lookup_prefix_cache(choices) {
            return (
                cached.status,
                cached.nodes,
                cached.spans,
                cached.observations,
            );
        }
        let ntc = NativeTestCase::for_choices(choices, None);
        let (status, nodes, spans, observations) = self.run_on(ntc);
        self.try_cache(choices, &status, &nodes, &spans, &observations);
        (status, nodes, spans, observations)
    }

    /// Cache a deterministic test run (one where `nodes.len() <= choices.len()`,
    /// i.e. the test didn't draw past the supplied prefix). We key by the
    /// input's first `nodes.len()` entries — the "used prefix" — so any later
    /// input whose first draws match replays identically and hits the cache.
    fn try_cache(
        &mut self,
        choices: &[ChoiceValue],
        status: &Status,
        nodes: &[ChoiceNode],
        spans: &[Span],
        observations: &HashMap<String, f64>,
    ) {
        if nodes.len() > choices.len() {
            return;
        }
        let key = encode_choice_key(&choices[..nodes.len()]);
        self.cache.insert(
            key,
            CachedRun {
                status: *status,
                nodes: nodes.to_vec(),
                spans: spans.to_vec(),
                observations: observations.clone(),
            },
        );
    }

    /// Port of `ConjectureRunner.optimise_targets`: run `Optimiser` for each
    /// target in a ramp-up schedule until no progress is made or the budget
    /// is exhausted.
    pub fn optimise_targets(&mut self) -> Result<(), RunIsComplete> {
        let mut max_improvements: usize = 10;
        loop {
            let prev_calls = self.valid_examples;
            let mut any_improvements = false;

            let targets: Vec<String> = self.best_observed_targets.keys().cloned().collect();
            for target in &targets {
                if self.valid_examples >= self.max_examples {
                    return Err(RunIsComplete);
                }
                let imps = self.hill_climb(target, max_improvements)?;
                if imps > 0 {
                    any_improvements = true;
                }
            }

            max_improvements = max_improvements.saturating_mul(2);

            if any_improvements {
                continue;
            }
            if prev_calls == self.valid_examples {
                break;
            }
        }
        Ok(())
    }

    /// Port of `Optimiser.hill_climb` for a single target key.
    fn hill_climb(
        &mut self,
        target: &str,
        max_improvements: usize,
    ) -> Result<usize, RunIsComplete> {
        let start_choices = match self.best_choices_for_target.get(target).cloned() {
            Some(c) => c,
            None => return Ok(0),
        };
        let (status, mut current_nodes, mut current_spans, mut current_obs) =
            self.run_extend_full(&start_choices);
        if status < Status::Valid {
            return Ok(0);
        }
        let mut current_score = *current_obs.get(target).unwrap_or(&f64::NEG_INFINITY);
        let mut improvements: usize = 0;

        let mut nodes_examined: HashSet<usize> = HashSet::new();
        let mut i: isize = current_nodes.len() as isize - 1;
        let mut prev_len = current_nodes.len();

        while i >= 0 && improvements <= max_improvements {
            if self.valid_examples >= self.max_examples {
                return Err(RunIsComplete);
            }
            if current_nodes.len() != prev_len {
                i = current_nodes.len() as isize - 1;
                prev_len = current_nodes.len();
                nodes_examined.clear();
            }
            let idx = i as usize;
            if idx >= current_nodes.len() || nodes_examined.contains(&idx) {
                i -= 1;
                continue;
            }
            nodes_examined.insert(idx);

            let node = &current_nodes[idx];
            if !node.was_forced
                && matches!(
                    node.kind,
                    ChoiceKind::Integer(_)
                        | ChoiceKind::Boolean(_)
                        | ChoiceKind::Bytes(_)
                        | ChoiceKind::Float(_)
                )
            {
                let len_before = current_nodes.len();
                self.find_integer(
                    target,
                    &mut current_nodes,
                    &mut current_spans,
                    &mut current_obs,
                    &mut current_score,
                    &mut improvements,
                    max_improvements,
                    idx,
                    1,
                )?;
                // If the `+1` direction grew current_nodes, the `idx` we were
                // examining is no longer at a "frontier" position — trying
                // `-1` there almost always shrinks the sequence back to a
                // lower score. Upstream calls it regardless but gets it for
                // free from the data tree; we pay a test call each time, so
                // skip when we can clearly see the probe won't pay off.
                if idx < current_nodes.len() && current_nodes.len() == len_before {
                    self.find_integer(
                        target,
                        &mut current_nodes,
                        &mut current_spans,
                        &mut current_obs,
                        &mut current_score,
                        &mut improvements,
                        max_improvements,
                        idx,
                        -1,
                    )?;
                }
            }

            i -= 1;
        }
        Ok(improvements)
    }

    /// Port of `junkdrawer.find_integer` specialised for a `try_replace`-style
    /// predicate: linear scan 1..5, then exponential-probe + binary-search.
    #[allow(clippy::too_many_arguments)]
    fn find_integer(
        &mut self,
        target: &str,
        current_nodes: &mut Vec<ChoiceNode>,
        current_spans: &mut Vec<Span>,
        current_obs: &mut HashMap<String, f64>,
        current_score: &mut f64,
        improvements: &mut usize,
        max_improvements: usize,
        idx: usize,
        sign: i64,
    ) -> Result<(), RunIsComplete> {
        for k in 1..5i64 {
            if self.valid_examples >= self.max_examples {
                return Err(RunIsComplete);
            }
            if *improvements > max_improvements {
                return Ok(());
            }
            if !self.try_replace(
                target,
                current_nodes,
                current_spans,
                current_obs,
                current_score,
                improvements,
                idx,
                sign * k,
            ) {
                return Ok(());
            }
        }

        let mut lo = 4i64;
        let mut hi = 5i64;
        loop {
            if self.valid_examples >= self.max_examples {
                return Err(RunIsComplete);
            }
            if *improvements > max_improvements {
                return Ok(());
            }
            if !self.try_replace(
                target,
                current_nodes,
                current_spans,
                current_obs,
                current_score,
                improvements,
                idx,
                sign * hi,
            ) {
                break;
            }
            lo = hi;
            hi = hi.saturating_mul(2);
            if hi > (1 << 20) {
                return Ok(());
            }
        }

        while lo + 1 < hi {
            if self.valid_examples >= self.max_examples {
                return Err(RunIsComplete);
            }
            if *improvements > max_improvements {
                return Ok(());
            }
            let mid = lo + (hi - lo) / 2;
            if self.try_replace(
                target,
                current_nodes,
                current_spans,
                current_obs,
                current_score,
                improvements,
                idx,
                sign * mid,
            ) {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        Ok(())
    }

    /// Port of `Optimiser.attempt_replace` + `consider_new_data`: build a
    /// candidate choice sequence by shifting `current_nodes[idx]` by `k`, run
    /// it extend="full", and commit if the target score did not decrease. If
    /// the raw attempt fails but the perturbation changed the size of a span
    /// covering `idx`, fall through to the span-fixup pass (splice the
    /// attempt's span contents into the current prefix + tail).
    ///
    /// Hypothesis wraps this in a 3-retry loop per `k` to let the random
    /// extension resample (via data-tree-backed novel-prefix sampling). We
    /// match the retry structure: up to 3 attempts per `k`, with early-outs
    /// on `EarlyStop` and on "no growth" (same node count as current), both
    /// of which signal that a fresh extension won't change anything. Each
    /// retry pulls a fresh seed from `self.rng` so the tail differs.
    #[allow(clippy::too_many_arguments)]
    fn try_replace(
        &mut self,
        target: &str,
        current_nodes: &mut Vec<ChoiceNode>,
        current_spans: &mut Vec<Span>,
        current_obs: &mut HashMap<String, f64>,
        current_score: &mut f64,
        improvements: &mut usize,
        idx: usize,
        k: i64,
    ) -> bool {
        if k.abs() > (1 << 20) {
            return false;
        }
        if idx >= current_nodes.len() {
            return false;
        }
        let node = current_nodes[idx].clone();
        if node.was_forced {
            return false;
        }

        let new_val = match (&node.value, &node.kind) {
            (ChoiceValue::Integer(v), ChoiceKind::Integer(kind)) => {
                let new = v.saturating_add(k as i128);
                if !kind.validate(new) {
                    return false;
                }
                ChoiceValue::Integer(new)
            }
            (ChoiceValue::Boolean(b), ChoiceKind::Boolean(_)) => {
                if k.abs() > 1 {
                    return false;
                }
                if k == -1 {
                    ChoiceValue::Boolean(false)
                } else if k == 1 {
                    ChoiceValue::Boolean(true)
                } else {
                    ChoiceValue::Boolean(*b)
                }
            }
            (ChoiceValue::Bytes(b), ChoiceKind::Bytes(kind)) => {
                let mut v: i128 = 0;
                for &byte in b {
                    v = (v << 8) | byte as i128;
                }
                let new_v = v + k as i128;
                if new_v < 0 {
                    return false;
                }
                let mut new_bytes = Vec::new();
                let mut x = new_v;
                if x == 0 {
                    new_bytes.push(0u8);
                }
                while x > 0 {
                    new_bytes.push((x & 0xff) as u8);
                    x >>= 8;
                }
                new_bytes.reverse();
                while new_bytes.len() < b.len() {
                    new_bytes.insert(0, 0);
                }
                if !kind.validate(&new_bytes) {
                    return false;
                }
                ChoiceValue::Bytes(new_bytes)
            }
            _ => return false,
        };

        let choices: Vec<ChoiceValue> = current_nodes
            .iter()
            .enumerate()
            .map(|(j, n)| {
                if j == idx {
                    new_val.clone()
                } else {
                    n.value.clone()
                }
            })
            .collect();

        for _ in 0..3 {
            if self.valid_examples >= self.max_examples {
                return false;
            }
            let (status, new_nodes, new_spans, new_obs) = self.run_extend_full(&choices);
            if Self::consider_new_data(
                target,
                current_nodes,
                current_spans,
                current_obs,
                current_score,
                improvements,
                status,
                new_nodes.clone(),
                new_spans.clone(),
                new_obs,
            ) {
                return true;
            }
            if status == Status::EarlyStop {
                return false;
            }
            if new_nodes.len() == current_nodes.len() {
                return false;
            }
            // Span-fixup: for each current span that brackets `idx`, if the
            // attempt's same-index span covers a different number of choices,
            // splice the attempt's span contents into the current prefix and
            // keep current's tail past the span. Port of the loop at the end
            // of Hypothesis's `Optimiser.attempt_replace`.
            let current_spans_snapshot = current_spans.clone();
            let current_values_snapshot: Vec<ChoiceValue> =
                current_nodes.iter().map(|n| n.value.clone()).collect();
            for (j, ex) in current_spans_snapshot.iter().enumerate() {
                if ex.start > idx {
                    break;
                }
                if ex.end <= idx {
                    continue;
                }
                let ex_attempt = match new_spans.get(j) {
                    Some(s) => s.clone(),
                    None => continue,
                };
                if ex.end - ex.start == ex_attempt.end - ex_attempt.start {
                    continue;
                }
                let replacement: Vec<ChoiceValue> = new_nodes[ex_attempt.start..ex_attempt.end]
                    .iter()
                    .map(|n| n.value.clone())
                    .collect();
                let mut spliced: Vec<ChoiceValue> = Vec::new();
                spliced.extend_from_slice(&current_values_snapshot[..idx]);
                spliced.extend(replacement);
                if ex.end < current_values_snapshot.len() {
                    spliced.extend_from_slice(&current_values_snapshot[ex.end..]);
                }
                if self.valid_examples >= self.max_examples {
                    return false;
                }
                let (s_status, s_nodes, s_spans, s_obs) = self.run_exact(&spliced);
                if Self::consider_new_data(
                    target,
                    current_nodes,
                    current_spans,
                    current_obs,
                    current_score,
                    improvements,
                    s_status,
                    s_nodes,
                    s_spans,
                    s_obs,
                ) {
                    return true;
                }
            }
            // Retrying with a fresh random tail only makes sense when the
            // attempt's random extension past the input prefix might resample
            // to a luckier outcome. If the attempt was SHORTER than current
            // (test mark_invalid'd before finishing) the test didn't draw past
            // the prefix, so there's no tail to resample.
            if new_nodes.len() < current_nodes.len() {
                return false;
            }
        }
        false
    }

    /// Port of `Optimiser.consider_new_data`. Returns true iff the candidate
    /// overwrites the current state (either strict improvement or a lateral
    /// move that does not grow the node count).
    #[allow(clippy::too_many_arguments)]
    fn consider_new_data(
        target: &str,
        current_nodes: &mut Vec<ChoiceNode>,
        current_spans: &mut Vec<Span>,
        current_obs: &mut HashMap<String, f64>,
        current_score: &mut f64,
        improvements: &mut usize,
        new_status: Status,
        new_nodes: Vec<ChoiceNode>,
        new_spans: Vec<Span>,
        new_obs: HashMap<String, f64>,
    ) -> bool {
        if new_status < Status::Valid {
            return false;
        }
        let new_score = *new_obs.get(target).unwrap_or(&f64::NEG_INFINITY);
        if new_score < *current_score {
            return false;
        }
        if new_score > *current_score {
            *current_score = new_score;
            *current_nodes = new_nodes;
            *current_spans = new_spans;
            *current_obs = new_obs;
            *improvements += 1;
            return true;
        }
        if new_nodes.len() <= current_nodes.len() {
            *current_nodes = new_nodes;
            *current_spans = new_spans;
            *current_obs = new_obs;
            return true;
        }
        false
    }

    pub fn best_observed_targets(&self) -> &HashMap<String, f64> {
        &self.best_observed_targets
    }
}

/// RAII guard that temporarily lowers the native engine's buffer size limit.
/// Port of `tests/conjecture/common.py::buffer_size_limit`.
pub struct BufferSizeLimit {
    prev: Option<usize>,
}

impl BufferSizeLimit {
    pub fn new(n: usize) -> Self {
        let prev = BUFFER_SIZE_OVERRIDE.with(|c| {
            let old = *c.borrow();
            *c.borrow_mut() = Some(n);
            old
        });
        BufferSizeLimit { prev }
    }
}

impl Drop for BufferSizeLimit {
    fn drop(&mut self) {
        let prev = self.prev;
        BUFFER_SIZE_OVERRIDE.with(|c| *c.borrow_mut() = prev);
    }
}
