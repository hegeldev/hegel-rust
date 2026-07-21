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

#[hegel::test(nondeterministic = true)]
fn test_kv_store(tc: TestCase) {
    let m = KVTest { placeholder: 0 };
    hegel::stateful::run_concurrent(m, tc, 3); // The three here is the maximum concurrency level.
}
```

The model is shared by reference across the worker threads, so rules and invariants take `&self` and the model type must be `Sync + Send`; any mutable model state needs interior mutability (locks, atomics, etc.). This also means the existing `StateMachine` trait (whose rules take `&mut self`) can't be reused: `#[concurrent_state_machine]` generates an impl of a new `ConcurrentStateMachine` trait carrying the rules, their group assignments, and the invariants.

The overlap semantics of groups, precisely: at any moment exactly one group is *current*, and only rules belonging to the current group are handed out — so rules in the same group may run concurrently with each other, rules in different groups never overlap, and the current group changes only at the join points (via `next_group`, below). Groups cannot express asymmetric overlap ("put may overlap get but not delete"); that expressiveness limit is deliberate and gets a rustdoc note.

A `#[rule]` with no `group = ...` argument is assigned to a single shared anonymous group, so an unannotated machine is maximally concurrent: any rule may overlap with any other, and naming groups is how overlap gets *restricted*. One consequence must be documented loudly (in the `#[concurrent_state_machine]` rustdoc and the doc example): in a machine that mixes annotated and unannotated rules, the unannotated rules form their own group and therefore never overlap with any named group's rules — the natural misreading of "no group" as "unconstrained" is exactly backwards there. The anonymous group is a frontend concept — the macro synthesizes a group entry for it when at least one rule uses it — and the engine only ever sees group indices.

## Declaring nondeterminism

A test that uses `run_concurrent` must declare itself nondeterministic *statically*, via `#[hegel::test(nondeterministic = true)]` (with a corresponding `nondeterministic(bool)` builder method on `Settings` for non-macro users, who pass it through `Hegel::settings` like any other setting). The method must live on `Settings`, not on the `Hegel` builder: the attribute's `key = value` pairs are chained as builder calls onto `::hegel::Settings::new()` (see `SettingsAttrArgs::to_settings_expr`), so that is where the macro form resolves. The `key = value` form is deliberate: the `#[hegel::test]` attribute grammar accepts either a leading settings *expression* or `key = value` pairs mapped to `Settings` builder methods, so a bare `nondeterministic` ident would parse as a settings expression and fail to resolve (or worse, silently resolve to an in-scope variable of that name); `nondeterministic = true` rides the existing machinery with no new parsing. The declaration is what lets the frontend decide, before any test case runs, that emission should be buffered and that no final replay will happen, and lets it configure the engine accordingly when the run starts.

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

Before running any rules, each worker must enter the test context: the worker's rule loop runs inside `with_test_context` and sets `CAPTURE_BACKTRACE`. Both are thread-locals that do not propagate to spawned threads, and the panic hook captures nothing on a thread that is outside the test context — it just forwards to the previous hook, spewing the raw panic to stderr — so without this setup the worker-side capture that the ferrying below relies on would never happen. Entering the test context on workers also makes `hegel_internal_assert!` / `raise_internal_error` raise a catchable `InternalError` control payload there instead of a plain panic, matching main-thread behavior.

Each worker then runs each rule under `catch_unwind`, mirroring the sequential runner's per-rule handling:

- `AssumeFailed`: note it and continue with the next rule, matching sequential `run`. (An assume-abandoned rule can poison locks it was holding and induce fake panics in other workers — see "Abandoned rules and lock poisoning" below.)
- `StopTest` (out of choice budget — the budget is family-wide, so one worker overrunning makes the other streams' draws fail too): the worker ends its participation and signals the main thread; the whole case is reported as an overrun. Overrun takes precedence over any panic in the same round (see below).
- `InternalError` / `InvalidArgument`: because workers run inside the test context, these run-aborting control payloads are raised catchably on workers too. The worker ferries the payload to the main thread verbatim and stops running rules for this case; the main thread `resume_unwind`s it with no panic-info re-install — control unwinds skip the panic hook, so there is no capture to ferry, and treating one as a user panic would misreport a framework/usage error as a test failure.
- A real panic: the panic hook has already stored the panic's thread name / location / backtrace in the *worker's* thread-local slot. The worker reads it there and ferries `(payload, panic info)` to the main thread, then stops running rules for this case.

At the join point, precedence is: **control payloads win over overrun, and overrun wins over panic**. If any worker ferried an `InternalError` or `InvalidArgument`, the main thread skips the invariants, signals termination, joins the workers, and re-raises that payload (lowest thread index wins if several arrive in one round): these signal a framework or usage bug that must not be masked, and unlike a panic neither can be induced by another worker's unwind side effects, so they are trustworthy even alongside an overrun. Otherwise, if any worker stopped with `StopTest`, the main thread skips the invariants, signals termination, joins the workers, and re-raises `StopTest`, so the whole case is classified as an overrun — even if another worker panicked in the same round. A panic that co-occurs with an overrun is not trustworthy: the overrunning worker's rule was abandoned mid-execution by an unwind, and that unwind's side effects can induce panics in other workers that no real schedule of the user's rules could produce (canonically: the abandoned rule was holding a `std::sync::Mutex` guarding the shared model, the `StopTest` unwind poisons it, and another worker's `lock().unwrap()` panics with `PoisonError`). The costs of misclassifying are also asymmetric: reporting a fake panic halts generation with a false bug as the run's single reported failure, while classifying the case as an overrun merely discards it — generation continues, and a genuine racy panic will resurface in a later case that doesn't overrun. The dropped panic is not silently swallowed: its rendered message is appended to the case's output buffer, so it stays visible whenever that buffer is shown. There is no ordering refinement to this rule — within a round there is no reliable happens-before between one worker's budget exhaustion and another's panic — so it keys off round-level co-occurrence only.

Otherwise, if any worker panicked, the main thread skips the invariants, signals termination, joins the workers, re-installs the ferried panic info into *its own* thread-local panic-info slot, and `resume_unwind`s the payload. (The worker-side half of the capture is the test-context setup above; this main-thread re-install half needs a `pub(crate)` setter in `run_lifecycle`.) Re-installing the info is what gives the failure its real origin (`Panic at file:line:col` from the worker's panic site) instead of the current `Panic at <unknown>` fallback for cross-thread panics — so distinct concurrent bugs keep distinct origins. If several workers panic in the same round, the lowest thread index wins; the losers' panics are dropped (their notes up to the panic are still in the case buffer).

The ferry-level precedence above is backed by a classification-level invariant: **what the frontend reports as the run's failure must agree with the engine family's write-once conclusion**. `FamilyCore::conclude` is first-writer-wins, and the overrunning draw has already concluded the family `EarlyStop` by the time the panic is being classified — so overrun-beats-panic is not just epistemically right but structurally forced: a `mark_complete(INTERESTING)` after it is silently ignored (a documented family-wide no-op) and the engine still counts the case as an overrun. Overrun is also not the only mid-round engine-side conclusion. A worker's draw can conclude the family `Invalid` (span nesting past `MAX_DEPTH`, clones past `MAX_CLONE_DEPTH`), and that surfaces to the drawing worker as an ordinary assume failure — indistinguishable at the frontend from a routine `AssumeFailed` — so no ferry-level rule can catch it, and a panic in the same round (possibly *induced by* the invalidated rule's abandonment, per the same argument as overrun) wins the ferry precedence, reports INTERESTING, and loses silently. Left alone, that would let the frontend print a failure block for a run the engine ultimately reports as passing.

No new ABI is needed to reconcile this: the run verdict already carries the answer at the only moment the frontend acts on it. The frontend defers the print decision to the end of the run (see "Capture everything at discovery time"): a case that classifies interesting frontend-side only *retains* its buffer, diagnostic, and payload as the run's stashed candidate, and the stash is printed and re-raised only if the run result comes back failed. That is sound because the engine concludes `Interesting` only via `mark_complete`, and generation stops at the first accepted bug, so no test case runs after it: accepted-interesting ⊆ frontend-interesting, and when the run fails, the last stash *is* the accepted bug. When a losing report is never followed by a real bug, the run passes and the stash is discarded — the same asymmetry the overrun rule leans on: a misclassified case is merely discarded, and a genuine racy panic resurfaces in a later case.

There is no cancellation: workers that neither panicked nor stopped finish their rule streams for the round normally.

### Abandoned rules and lock poisoning

The rationale behind overrun-beats-panic — an unwind abandons a rule mid-execution, and the abandonment's side effects induce panics in other workers that no real schedule of the user's rules could produce — is not specific to overrun. A routine `AssumeFailed` abandons a rule the same way, and rules are *expected* to reject: an empty-pool draw is an assume violation by design, and `tc.assume` inside a rule is ordinary usage. A rule that locks the shared model and then draws can unwind while holding the guard, poisoning a `std::sync::Mutex`; every other worker's `lock().unwrap()` then panics with `PoisonError`. With no engine-side conclusion in the round, that fake panic wins classification and halts the run as its single reported failure — and even alongside a *genuine* panic, a poisoning cascade can steal the report (lowest thread index wins) or bury it in noise.

No precedence rule can fix this one: assume failures are far too routine for "any assume in the round suppresses panics" — that would drown exactly the genuine racy panics this feature exists to find. The mitigation lives in the model, and the documentation must own it. The `#[concurrent_state_machine]` / `run_concurrent` rustdoc and the doc example direct users to:

- **Draw before locking**: complete all of a rule's draws (including pool draws, which reject when empty) before taking any lock, so a rejection can never unwind through a held guard.
- **Make model locks poison-tolerant**: recover with `unwrap_or_else(|e| e.into_inner())` — the same idiom the engine uses on its family locks and `ConcurrentPool` uses on its map — or use a non-poisoning lock (e.g. `parking_lot`). Then even a mid-rule *panic* can't turn later lock acquisitions into fake `PoisonError` failures; the half-mutated state a recovered lock may expose is precisely what the invariants are there to catch, and the original panic is still the one reported.

`ConcurrentPool` itself already follows both rules (its empty-pool rejection is raised only after its guard is dropped, and every acquisition recovers from poisoning), so the pattern users are told to follow is the one the framework's own concurrent structure uses.

### Main-thread unwinds

The worker-side failures above are ferried to the main thread and re-raised there, but the main thread also has unwind paths of its own, all at the join points: an invariant panics, an invariant raises `AssumeFailed` (making the whole case invalid), a draw inside `next_group` or an invariant exhausts the family budget (`StopTest`), or an internal error is raised. Every one of these fires while the workers are parked waiting for their next wake signal, and an unwind that leaves the `thread::scope` body would block forever in the scope's implicit join — the parked workers never exit, so the scope never closes. (The deadlock non-goal above is about deadlocks in the SUT; this one is deterministic framework behavior — an invariant drawing past the budget is routine — and must not hang.)

`run_concurrent` therefore installs a termination guard for the lifetime of the scope body: a `Drop` impl that signals termination to the workers — the same signal the normal end-of-case path sends, made idempotent so double-signaling is harmless. Any unwind then wakes the parked workers, they exit their loops, the scope join completes, and the unwind propagates out of `run_concurrent` to be classified by the lifecycle as usual (panic → interesting, `AssumeFailed` → invalid, `StopTest` → overrun). The explicit signal-then-join sequences in the worker-failure paths above remain as written; the guard is the backstop that makes every exit path — including ones added later — safe by construction.

## Pools

Pools are supported in concurrent machines, and workers may share one. Engine-side, nothing changes: pools are already family-wide (shared across all clone streams behind the family's `variable_pools` mutex), and `pool_generate` already performs its empty check, selection draw, and consume atomically under that lock — returning an assume-violation when the pool is empty — so concurrent workers drawing from one pool cannot double-consume a variable id or race the emptiness check.

The accepted trade-off: a shared pool couples the workers' streams. The selection index is drawn from the calling worker's own stream, but which variable id that index resolves to — and whether the draw rejects as empty — depends on what other workers have added or consumed in the meantime. This is a deliberate exception to "draws on one thread never affect draws on another": the convenience of a shared pool is worth the loss of stream independence, and in a nondeterministic run (the only place concurrency > 1 exists) nothing downstream depends on that independence anyway — there is no replay or shrinking to confuse.

Frontend-side, the existing `Pool<T>` stays exactly as it is for sequential machines, but it cannot cross threads: `add` and `values_consumed` take `&mut self`, `values_reusable` hands out `&T` borrows of the id→value map, and the stored `TestCase` makes it `!Sync`. Concurrent machines get a new `ConcurrentPool<T>` (created with `stateful::concurrent_pool::<T>(&tc)`), designed for `&self` access from rules:

- The id→value map lives behind a `Mutex`, and the pool stores no `TestCase` — `add(&self, tc: &TestCase, v: T)` takes the calling worker's handle explicitly — so `ConcurrentPool<T>` is `Sync` whenever `T: Send`, and a model holding one satisfies the `Sync + Send` bound.
- `values_consumed(&self)` yields `T` by value: the engine consumes the id atomically, so exactly one worker receives each value.
- `values_reusable(&self)` yields owned clones (`T: Clone` bound on the method) rather than `&T`: another worker may consume the referenced value at any moment, so references cannot safely escape the lock. Users with expensive values can store `Arc<T>` in the pool.
- Every pool operation (add and both generators' draws) holds the pool's frontend mutex across its engine call and its map access, keeping the frontend map in lockstep with the engine's pool state. Without this, a consumer can be handed a variable id whose value the adding worker hasn't inserted into the map yet, and a reusable draw can look up an id another worker just consumed — both of which would panic on a failed lookup. Lock order is always frontend pool lock → engine family lock (the engine releases its lock before returning), so no deadlock is possible; an empty-pool rejection must be raised as `AssumeFailed` only after the frontend guard is dropped, so the unwind does not poison the mutex.
- Every acquisition of the pool's frontend mutex recovers from poisoning (`unwrap_or_else(|e| e.into_inner())`, the same idiom the engine uses on all its family locks) rather than `unwrap()`ing. A panic can still unwind while the guard is held — canonically a user's `Clone` impl panicking inside `values_reusable`, which must clone under the lock since another worker may consume the value the moment the guard drops — and with plain `unwrap()` that one panic would poison the mutex and turn every later pool operation on every worker into a `PoisonError` panic, burying the real failure under fake ones. Recovery is sound here because no panic point sits between map mutations: a panicking clone reads the map without modifying it, and `add` inserts with an engine-issued id in a single operation, so the guarded map is consistent whenever the guard is droppable.

## Never replay, never shrink

Concurrent stateful tests, unlike any others, should never do replay and shrinking. We want the simplest thing that reports failures faithfully without flakiness complaints. That splits into an engine half and a frontend half.

### Engine side: one settings flag

A new settings-level flag (see `hegel_settings_set_nondeterministic` below) tells the engine the run is nondeterministic. When set, the engine skips, run-wide:

- **data-tree recording** — which also disables novel-prefix generation and the choice-tree mismatch check (the source of `NonDeterministic` run errors), since the tree stays empty
- **span mutation** — it replays choice sequences during the generate phase
- **the verify+shrink block per origin** — the engine behaves as if the Shrink phase were disabled; this is also what removes the engine-side `Flaky` error
- **targeting** — as if the Target phase were disabled
- **database persistence and reuse**
- **reproduce-blob emission** — failures are reported with `reproduce_blob: None`, where today the engine unconditionally attaches an encoded blob to every failure it reports

Keying all of this off one engine-side flag (rather than having the frontend fiddle with phases) keeps frontends simple and enforces the invariant "nondeterministic ⇒ none of these run" in one place; whatever phases the user configured are left alone and simply don't take effect.

A consequence to document: with shrinking disabled, `should_generate_more` stops generation at the first bug, so a nondeterministic run reports at most one failure.

### Frontend side: capture everything at discovery time

All failure information is captured during the bug-discovering run; no replay is necessary:

- Up-front capture applies only to runs declared nondeterministic; deterministic runs keep today's replay-time emission and pay nothing new. Every test case of a nondeterministic run executes with emission on, into a per-case buffer containing the standard draw-line representation of the choice sequence interleaved with notes. Buffered lines are tagged with the worker's thread index so interleaved output stays readable. Within such a run the per-case formatting cost is inherent — with no replay, discovery time is the only chance to capture the output — and bounded: each buffer is dropped as soon as its case completes non-interesting, and generation stops at the first bug. Under `Verbosity::Quiet` nothing would be printed, so the buffering is skipped entirely there. Backtrace capture is also enabled for every case — affordable since a backtrace is only actually captured when a panic happens, and there are no shrink probes.
- When a case classifies interesting frontend-side, the frontend *retains* — rather than prints — the case's buffer, the rendered panic diagnostic (thread, location, message, backtrace), and the caught panic payload, as the run's stashed candidate (overwriting any previous stash). The print decision is deferred to the run verdict: if the run result comes back failed, the stash is necessarily the accepted bug (see "Worker panic handling") and is printed then — buffer followed by diagnostic — before the payload is re-raised; if the run passes, the stash was a report that lost to an engine-side family conclusion, and it is discarded. Discovery time remains the only chance to *capture* (there is no replay); deferring the print costs nothing observable, since generation stops at the first accepted bug and the run ends immediately after it.
- The failures in the run result carry no reproduce blob (`reproduce_blob` is already optional in the ABI — single-test-case runs use `None` today). For a declared-nondeterministic run, `drive` skips the final replay and its flaky check entirely and finishes by `resume_unwind`ing the stashed payload.
- No reproducer line is printed, and `#[reproduce_failure]` is not supported. The guard cannot live in `run_concurrent`: nothing visible to the test body distinguishes a blob replay from a normal run (`TestCase` exposes only `mode`, and the nondeterminism declaration is static on the test, so it is present in the replay run too). Instead `Hegel::run` panics up-front with a clear message when a reproduce blob is set on a run declared nondeterministic, before any test case executes.

## Engine-side state machine restructuring

`NativeStateMachine` currently keeps a single shared step cap / step count and a single set of swarm feature flags, drawn from whichever stream happens to call. The concurrent design requires:

- **Per machine** (owned by the root, advanced by `next_group` on the root handle): the current concurrency group and a drawn cap on the number of rounds.
- **Per thread index**: swarm feature flags and per-round step caps, all drawn from the calling handle's stream — so `next_rule` consults only per-thread and per-clone state, and draws on one thread never affect draws on another. (Shared pools are the deliberate exception to this independence; see "Pools".)
- `next_rule` returns only rules belonging to the current group, and group membership is never enforced by rejection sampling: the machine partitions the rules into per-group lists at creation, and every selection draw ranges over the current group's list only — an index in `[0, group_size)`, mapped back to the global rule index — so each draw is in-group by construction. Sampling over the full rule set and rejecting out-of-group results would waste draws and, for a small group among many rules, degrade most selections into the fallback path. The swarm enabled/disabled selection (the existing tries-then-fallback scheme) operates within the group's list. The swarm "at least one rule enabled" guarantee applies within each group, so every round can make progress. This requires restructuring the `FeatureFlags` bookkeeping, not just duplicating it per thread: today a single `at_least_one_of` set spans all rules, and with lazily-decided per-thread flags a thread could satisfy that global guarantee by enabling a rule in group A while disabling every rule of group B — leaving `select_rule` with an empty `allowed` set (an internal assertion failure) whenever B is the current group. Instead, each thread keeps one `at_least_one_of` set per group, and the "last undecided candidate is forced enabled" rule applies within each group's set.
- Step budget: each thread draws its per-round step cap from its own stream; `next_group` draws the round cap from the root stream. At concurrency 1 the engine does not draw a per-round step cap at all: it hands out exactly one rule per round, so every rule is followed by a join point and the frontend's join-point invariant checks run after each rule, exactly as sequential invariants do today. The round cap alone carries the step budget there, drawn so that a sequential test (concurrency 1, one group) gets roughly the same total number of steps as today's cap (≤ 50).
- In single-test-case mode (steps unbounded, e.g. under Antithesis), `next_group` never sets `*out_continue = false`: rounds continue forever, which is the intent.

## Sequential compatibility

Sequential stateful tests are just special-case usage of the new interface: call `hegel_new_state_machine` with a concurrency level of one and put all the rules in a single group. The frontend's `stateful::run` is rewritten to the new shape — an outer `next_group` loop around the existing `next_rule` loop, with the per-rule invariant calls moved to the join point — while keeping its initial invariant check and overall step guard. Invariant timing is preserved: because the engine emits exactly one rule per round at concurrency 1 (see "Step budget" above), a join point follows every rule, so invariants still run after each rule. One deliberate change: today's runner skips the invariant check when a rule raises `AssumeFailed`, and the join point runs invariants regardless. This is fine — rules are expected to fail their assumptions before mutating the model (nothing snapshots or restores model state), and today's skip only defers the check anyway: the loop continues against the same model state, so the next successful rule's invariant check sees whatever the assume-failed rule left behind. Note that sequential stateful tests are otherwise not required to work exactly as they do today.

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

There will be a settings-level flag declaring the run nondeterministic:

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
- `ConcurrentPool` is covered deterministically at concurrency 1 (add, reusable and consumed draws, empty-pool rejection) and exercised across workers by the deterministic-model orchestration tests, which is what pins the frontend-map-in-lockstep-with-engine invariant its lookups rely on.
- The run-verdict reconciliation is covered by deterministic injection: a case where a panic's INTERESTING report loses to an engine-side `Invalid` conclusion (span nesting past `MAX_DEPTH` in the same round as the panic) must leave the run's verdict to the engine — a pass prints no failure block and discards the stash. (The overrun variant never reaches the stash: the ferry-level `StopTest` precedence classifies it Overrun frontend-side, and its tests pin that directly.)

New code is subject to the 100% line-coverage ratchet, so the worker panic paths need to be reachable by deterministic injection, not just by winning races.

## Open questions

- Health checks (TooSlow, TestCasesTooLarge, ...) currently still apply to nondeterministic runs. We'll decide later whether any of them should be disabled.
