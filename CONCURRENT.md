We want to add concurrent stateful testing to Hegel. This will work similarly to the existing stateful testing interface, but it will allow users to make *concurrent* actions against the system under test.

Generally, concurrency bugs will be nondeterministic due to thread scheduling outside of our control, etc. That's fine. With this feature, we aren't expecting deterministic reproduction or shrinking at all. Finding that a particular sequence of concurrent actions can *sometimes* produce a bug is already useful information. If you want deterministic replay, use Antithesis. Specifically, we want to do the bare minimum to stop Hegel from complaining about flakiness for concurrent stateful tests.

For libhegel, this will be a breaking change, replacing the current stateful testing interface. Sequential stateful testing will be a special case of the new interface.

For the Rust frontend, this will be a non-breaking change; there will be a new concurrent stateful interface in addition to the existing sequential one. Here's a sketch of what consumption of the interface will look like, for the Rust frontend:

```rust
// Imagine we're testing a KV store. Say put, get, and delete may run
// concurrently, but dump can only overlap with itself.

use hegel::TestCase;

struct KVTest {
    placeholder: i32,
}

#[hegel::concurrent_state_machine]
impl KVTest {

    #[rule(group = "rw")]
    fn put(&self, tc: TestCase) {
        // do put here...
    }

    #[rule(group = "rw")]
    fn get(&self, tc: TestCase) {
        // do get here...
    }

    #[rule(group = "rw")]
    fn delete(&self, tc: TestCase) {
        // do delete here...
    }

    #[rule(group = "dump")]
    fn dump(&self, tc: TestCase) {
        // dump database here...
    }

    #[invariant]
    fn consistent(&self, tc: TestCase) {
        // check consistency here...
    }

}

#[hegel::test(nondeterministic)]
fn test_kv_store(tc: TestCase) {
    let m = KVTest { placeholder: 0 };
    hegel::stateful::run_concurrent(m, tc, 3); // The three here is the maximum concurrency level.
}
```

The model is shared by reference across the worker threads, so rules and invariants take `&self` and the model type must be `Sync + Send`; any mutable model state needs interior mutability (locks, atomics, etc.). This also means the existing `StateMachine` trait (whose rules take `&mut self`) can't be reused: `#[concurrent_state_machine]` generates an impl of a new `ConcurrentStateMachine` trait carrying the rules, their group assignments, and the invariants.

## Declaring nondeterminism

A test that uses `run_concurrent` must declare itself nondeterministic *statically*, via `#[hegel::test(nondeterministic)]` (with a corresponding setting on the `Hegel` builder for non-macro users). The declaration is what lets the frontend decide, before any test case runs, that emission should be buffered and that no final replay will happen, and lets it configure the engine accordingly when the run starts.

`run_concurrent` panics with a clear error message if the current run has not been declared nondeterministic, rather than letting the run proceed and fail later with a confusing flakiness complaint.

The declaration applies to the whole run — nondeterminism is a run-level property, not a per-test-case one. Even a test case that happens to draw a concurrency level of 1 gets no shrinking, replay, or persistence. (We considered marking nondeterminism per test case, so that drawn-concurrency-1 cases would stay shrinkable, but per-case granularity infects everything downstream — per-origin shrink decisions, mixed data-tree contents, mixed persistence — and isn't worth it.)

## Execution model

From the point of view of the frontend, a concurrent stateful test will look roughly as follows:

On the main thread:
```
// First, set up state machine and worker threads.
while hegel_state_machine_next_group() != TERMINATE:
    // Signal to each worker thread that it should run.
    // Wait for each worker thread to indicate completion.
    // Run all invariants.
```

On the worker threads:
```
while true:

    // Wait for main thread to wake us.

    if main_thread_indicates_termination:
        break
    while next_rule() != TERMINATE:
        // Execute rule.
```

`run_concurrent` spawns `concurrency` scoped worker threads once per test case. Each worker owns its own `TestCase` clone (an independent choice stream) for the whole test case; the main thread keeps the root handle, which is what `next_group` and the invariants draw from at the join points.

We won't try to handle deadlocks in the first pass at this. It's okay if the test suite hangs due to a deadlock in the SUT.

### Worker panic handling

Each worker runs each rule under `catch_unwind`, mirroring the sequential runner's per-rule handling:

- `AssumeFailed`: note it and continue with the next rule, matching sequential `run`.
- `StopTest` (out of choice budget — the budget is family-wide, so one worker overrunning makes the other streams' draws fail too): the worker ends its participation and signals the main thread; the whole case is reported as an overrun.
- A real panic: the panic hook has already stored the panic's thread name / location / backtrace in the *worker's* thread-local slot. The worker reads it there and ferries `(payload, panic info)` to the main thread, then stops running rules for this case.

At the join point, if any worker panicked, the main thread skips the invariants, signals termination, joins the workers, re-installs the ferried panic info into *its own* thread-local panic-info slot (this needs a `pub(crate)` setter in `run_lifecycle`), and `resume_unwind`s the payload. Re-installing the info is what gives the failure its real origin (`Panic at file:line:col` from the worker's panic site) instead of the current `Panic at <unknown>` fallback for cross-thread panics — so distinct concurrent bugs keep distinct origins. If several workers panic in the same round, the lowest thread index wins; the losers' panics are dropped (their notes up to the panic are still in the case buffer).

There is no cancellation: workers that neither panicked nor stopped finish their rule streams for the round normally.

## Never replay, never shrink

Concurrent stateful tests, unlike any others, should never do replay and shrinking. We want the simplest thing that reports failures faithfully without flakiness complaints. That splits into an engine half and a frontend half.

### Engine side: one settings flag

A new settings-level flag (see `hegel_settings_set_nondeterministic` below) tells the engine the run is nondeterministic. When set, the engine skips, run-wide:

- **data-tree recording** — which also disables novel-prefix generation and the choice-tree mismatch check (the source of `NonDeterministic` run errors), since the tree stays empty
- **span mutation** — it replays choice sequences during the generate phase
- **the verify+shrink block per origin** — the engine behaves as if the Shrink phase were disabled; this is also what removes the engine-side `Flaky` error
- **targeting** — as if the Target phase were disabled
- **database persistence and reuse**

Keying all of this off one engine-side flag (rather than having the frontend fiddle with phases) keeps frontends simple and enforces the invariant "nondeterministic ⇒ none of these run" in one place; whatever phases the user configured are left alone and simply don't take effect.

A consequence to document: with shrinking disabled, `should_generate_more` stops generation at the first bug, so a nondeterministic run reports at most one failure.

### Frontend side: capture everything at discovery time

All failure information is captured during the bug-discovering run; no replay is necessary:

- Every test case of a nondeterministic run executes with emission on, into a per-case buffer containing the standard draw-line representation of the choice sequence interleaved with notes. Buffered lines are tagged with the worker's thread index so interleaved output stays readable. Backtrace capture is also enabled for every case — affordable since there are no shrink probes.
- When a case comes back interesting, the frontend prints, at discovery time, the case's buffer followed by the rendered panic diagnostic (thread, location, message, backtrace), and stashes the caught panic payload.
- The failures in the run result carry no reproduce blob (`reproduce_blob` is already optional in the ABI — single-test-case runs use `None` today). For a declared-nondeterministic run, `drive` skips the final replay and its flaky check entirely and finishes by `resume_unwind`ing the stashed payload.
- No reproducer line is printed, and `#[reproduce_failure]` is not supported: a failure-blob replay that reaches `run_concurrent` panics with a clear message.

## Engine-side state machine restructuring

`NativeStateMachine` currently keeps a single shared step cap / step count and a single set of swarm feature flags, drawn from whichever stream happens to call. The concurrent design requires:

- **Per machine** (owned by the root, advanced by `next_group` on the root handle): the current concurrency group and a drawn cap on the number of rounds.
- **Per thread index**: swarm feature flags and per-round step caps, all drawn from the calling handle's stream — so `next_rule` consults only per-thread and per-clone state, and draws on one thread never affect draws on another.
- `next_rule` returns only rules belonging to the current group. The swarm "at least one rule enabled" guarantee applies within each group, so every round can make progress.
- Step budget: each thread draws its per-round step cap from its own stream; `next_group` draws the round cap from the root stream. Constants are tuned so that a sequential test (concurrency 1, one group) hands out roughly the same total number of steps as today's cap (≤ 50).
- In single-test-case mode (steps unbounded, e.g. under Antithesis), `next_group` never sets `*out_continue = false` and `next_rule` never returns the sentinel: rounds continue forever, which is the intent.

## Sequential compatibility

Sequential stateful tests are just special-case usage of the new interface: call `hegel_new_state_machine` with a concurrency level of one and put all the rules in a single group. The frontend's `stateful::run` is rewritten to the new shape — an outer `next_group` loop around the existing `next_rule` loop — while keeping its overall step guard. The hard requirement: at concurrency 1, determinism, shrinking, swarm behavior, replay, and persistence must all keep working exactly as they do today, since existing sequential tests (which do not declare nondeterminism) go through the new code path with everything enabled.

The choice-sequence shape changes for sequential tests (`next_group` consumes draws that didn't exist before), which invalidates existing stored database entries and `#[reproduce_failure]` blobs for stateful tests. Stale database entries will replay as invalid/overrun and be deleted quietly; stale blobs will fail loudly. This needs a release note.

## Changes to libhegel

The libhegel interfaces will have the following changes. (Remember to regenerate `hegel-c/include/hegel.h` with `just c-header`; downstream language bindings consuming the released `libhegel-*` assets will need updating.)

There will be a primitive call to request a concurrency level, up to a maximum:

```rust
pub unsafe extern "C" fn hegel_generate_concurrency(
    ctx: *mut HegelContext,
    tc: *mut HegelTestCase,
    max_value: i64,
    out_value: *mut i64,
) -> hegel_result_t
```

`max_value` must be ≥ 1. The engine owns the distribution; it should be weighted toward `max_value` (concurrency bugs need concurrency) rather than shrink-biased toward 1, which is why this is a dedicated primitive instead of a plain integer draw.

The return value should be passed as the concurrency level when creating a new state machine:

```rust
pub unsafe extern "C" fn hegel_new_state_machine(
    ctx: *mut HegelContext,
    tc: *mut HegelTestCase,
    group_names: *const *const c_char,
    num_groups: usize,
    rule_names: *const *const c_char,
    rule_groups: *const i64,
    num_rules: usize,
    invariant_names: *const *const c_char,
    num_invariants: usize,
    concurrency: i64,
    out_state_machine_id: *mut i64,
) -> hegel_result_t
```

`group_names` is an array (length `num_groups`) of concurrency group names. `rule_groups` is an array of concurrency group indices, parallel to `rule_names`; each entry must be in `[0, num_groups)`. `num_groups` and `num_rules` must be ≥ 1, and `concurrency` must be ≥ 1.

There will be a settings-level flag declaring the run nondeterministic (this replaces the per-test-case `hegel_mark_nondeterministic` from an earlier draft — since nondeterminism is a sticky, run-level property declared statically by the frontend, a settings flag is the natural home):

```rust
pub unsafe extern "C" fn hegel_settings_set_nondeterministic(
    ctx: *mut HegelContext,
    s: *mut HegelSettings,
    nondeterministic: bool,
) -> hegel_result_t
```

It's the responsibility of the frontend to set this whenever a run may be nondeterministic. Most of the time, that means whenever the test declares it (because it uses concurrent stateful testing), unless the frontend is doing some kind of custom deterministic scheduling. The engine-side effects are described under "Never replay, never shrink" above; failures from such a run carry no reproduce blob.

There will be a new function to rerandomize the concurrency group. This should be called at the join points of a concurrent stateful test (after each worker thread has exhausted its rule stream), on the root test-case handle:

```rust
pub unsafe extern "C" fn hegel_state_machine_next_group(
    ctx: *mut HegelContext,
    tc: *mut HegelTestCase,
    state_machine_id: i64,
    out_continue: *mut bool,
) -> hegel_result_t
```

`out_continue` will be set to false to indicate termination.

The frontend will pass a thread index (which of the worker threads is the caller) when drawing a rule to run:

```rust
pub unsafe extern "C" fn hegel_state_machine_next_rule(
    ctx: *mut HegelContext,
    tc: *mut HegelTestCase,
    state_machine_id: i64,
    thread_index: i64,
    out_rule_index: *mut i64,
) -> hegel_result_t
```

`thread_index` must satisfy 0 <= `thread_index` < `concurrency` (passed at state machine creation). We pass a thread index to uniquely identify the thread because in theory a single thread could have multiple test case clones.

`out_rule_index` will be set to a sentinel value (-1) to indicate termination. This means the worker thread should wait for the next group / join point. The returned index is always a rule belonging to the current concurrency group.

Calls to `hegel_state_machine_next_rule` should only consult per-thread and per-clone state. That is, draws on one thread shouldn't affect draws on another.

Note that even for sequential tests it is the responsibility of the frontend to advance the group when the rule stream is exhausted, even though there's only a single group.

## Testing

- The engine-side round/group/step logic and the rewritten sequential frontend path are covered deterministically at concurrency 1 (this is also what pins the sequential-compatibility requirement).
- Worker orchestration (rounds, join points, assume/overrun/panic paths) is exercised with a deterministic model SUT — rules that don't actually race — so the panic-ferrying and termination paths are coverable without real nondeterminism.
- Concurrency > 1 bug-finding gets smoke tests that run a genuinely racy SUT but don't assert that the bug is found on any particular run.

New code is subject to the 100% line-coverage ratchet, so the worker panic paths need to be reachable by deterministic injection, not just by winning races.

## Open questions

- Health checks (TooSlow, TestCasesTooLarge, ...) currently still apply to nondeterministic runs. We'll decide later whether any of them should be disabled.
