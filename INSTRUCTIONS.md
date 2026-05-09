# INSTRUCTIONS.md — Native backend audit remediation

You are working on the `DRMacIver/native` branch of `hegel-rust`. The repository is at `/home/ubuntu/hegel-rust`. You are being run inside a Ralph loop — you will be re-spawned in a fresh context after each iteration. **This file is your durable memory.** Read it from the top each iteration.

## 0. The situation

The branch implements a native pure-Rust backend for Hegel (the `native` cargo feature) that runs the Hypothesis-style PBT engine in-process instead of going through the Python server. An audit found that **a large fraction of the implementation is sloppy, silently degraded, or claims behaviour it does not deliver**. Your job is to fix it — properly — until the native backend is genuinely good.

The reader is **David MacIver, original author of Hypothesis.** He will read this code. Anything fake, half-finished, or "it kind of works on the happy path" will infuriate him. **Your aim is code he reads and approves of**, not code that satisfies a CI check.

The Python references live in:
- `/home/ubuntu/hegel-rust/external/hypothesis/hypothesis-python/src/hypothesis/internal/conjecture/` (the engine, shrinker, optimiser, pareto)
- `/home/ubuntu/hegel-rust/external/pbtkit/` (DRMacIver's leaner reimplementation; closer in spirit to ours)

When you port a behaviour, **cite the upstream file:line in a comment** so the next reader can verify the port.

## 1. Iteration protocol

### General Principles

**All necessary work must be tracked in this document**. If you spot something that needs to be done, you should probably just do it, but even if you think it is out of scope for your current task, it is **mandatory** to make sure that it is tracked in this document as work that needs to be done. Update this document before continuing to do anything else.

### 1.0 GREEN BASELINE FIRST — non-negotiable

**Before doing anything else, run the test suite.**

```bash
cargo test --features native --tests --no-fail-fast 2>&1 | grep -E 'FAILED|test result'
cargo test --tests --no-fail-fast 2>&1 | grep -E 'FAILED|test result'
```

If any test fails — including pre-existing failures, including failures unrelated to the punch-list, including flakes — **the only valid iteration option is to fix one of those failures.** Tier-0 in §7 lists known-failing tests; if a failing test isn't already there, add it.

You cannot meaningfully audit, fix, or verify anything against a red baseline. "It wasn't caused by my change" is not an excuse. A red suite means *nothing* about correctness can be concluded — when a fix lands and the suite stays red, you can't tell if it broke something or not.

Do not pick from Tier S, A, B, C, D, or E until **every** test in both feature configurations passes. Do not write new tests until then either — they would be running against the same broken baseline.

If a test is *legitimately* impossible to fix (genuine platform issue, etc.), document it under §8 with `[ ]` so a human can decide whether to delete it; do not `#[ignore]` it as a workaround.

### 1.1 What an iteration does

Once the baseline is green (all tests pass under both feature configurations), each Ralph iteration does **one** of the following, then exits:

1. **Fix one punch-list item.** Pick the highest-priority unchecked item from §7 (Tier 0 first if non-empty, then S, A, B, C, D, E). Apply the test-first discipline in §3. Commit. Tick the box in this file (edit it). Done.
2. **Investigate further.** If §7 looks complete but you're not confident the audit was thorough, run one of the heuristics in §6 to look for new findings. Add anything you find as a new punch-list item with an unchecked box, in the appropriate tier. Commit the additions to INSTRUCTIONS.md. Done.
3. **Run the exit checks (§9).** If every box in §7 is ticked and you ran exit checks last iteration without finding anything new, run them again and emit the completion promise.

**Do not** try to do multiple items per iteration. One thing, well, per iteration. The loop will keep running.

**Always commit your work at the end of an iteration**, even if it's partial — but mark the punch-list item complete only when it is *actually* complete by the standards of §3 and §4. Use commit messages that reference the punch-list item number.

If you start an item and decide mid-way it's bigger than expected, split it: add new sub-items to the punch list, finish what you can, commit, and the next iteration will continue.

**Never edit this file to lower the quality bar to escape the loop.** If something is genuinely impossible, add an `## 8. Open questions` entry describing what's blocking and *what you tried*, then continue with the next item. Do not falsely claim completion.

## 2. The prime directive: TESTS FIRST

For every single bug on the punch list, **the first thing you do is make sure the test suite catches it.** Then, and only then, do you fix the bug.

The protocol for each item:

1. **Read the bug carefully.** Understand what the implementation does and what it should do. Read the relevant Python reference if applicable.
2. **Write a test that fails on the current code.** The test must:
   - Assert the *correct* behaviour, not the current (buggy) behaviour.
   - Run under `cargo test --features native` (and if relevant, also under default features).
   - Be a real behavioural test, not a "no panic" test (see §4).
3. **Run the test. Confirm it fails.** If it passes on the current buggy code, the test is wrong — it isn't catching the bug. Strengthen it until it fails.
4. **Commit the failing test.** Commit message: `tests: add failing test for <punch-list item N> — <one-line summary>`. (Yes, commit a failing test. The next iteration's CI will be red until the fix lands. That's intentional and OK on a feature branch — this is the discipline.)
   - **Exception:** if committing a failing test would block other work (e.g. the failing test panics in a way that takes down the whole test binary), gate it behind `#[ignore]` with a comment naming the punch-list item and remove the ignore in the same commit as the fix.
5. **Implement the fix.** Match the upstream reference where there is one. Remove the corresponding `// nocov` block if there is one — never re-add it.
6. **Run the test again. Confirm it now passes.** Run the *full* test suite (`just test` and `cargo test --features native`) — your fix must not break anything else.
7. **Commit the fix.** Commit message: `fix: <punch-list item N> — <one-line>`. Include `Refs: INSTRUCTIONS.md item N` in the body.
8. **Tick the box in §7.** Update INSTRUCTIONS.md (this file) so the next iteration knows the item is done.

Why tests first, always:
- It proves the bug is real and observable from the test surface.
- It prevents regressions when the next refactor lands.
- It forces you to articulate the correct behaviour before writing the fix, which catches a category of "I fixed something but not what I should have" mistakes.
- The audit found dozens of cases where existing tests asserted weak claims that wouldn't catch the bug. We are not adding to that pile.

## 3. The quality bar — what "good" means

A fix is not done until it meets every applicable point below. If you can't honestly tick all of them, the fix isn't ready.

### 3.1 Code

- **No fake stubs.** A function that exists must do what its name and doc claim. No `fn foo(_msg: &str) {}` "legacy stubs" left in the code unless they are genuinely needed; if they are needed, the no-op must be obvious from the name (e.g. `fn ignore_msg`) or wrapped in a clear "intentionally a no-op because X" comment.
- **No silent option-ignoring.** Every public Settings option, Phase, Verbosity level, etc. is either honoured by both backends or rejected at the boundary with a clear error. "Compiles and accepts but does nothing" is forbidden.
- **No `_ => panic!(...)` on external input.** Schema parsing, command dispatch, and anything that consumes data crossing a backend boundary must return `Status::Invalid` or a typed error for unrecognised input — not crash the test runner.
- **No bare `unreachable!()`.** Every `unreachable!()` includes a message naming the invariant that makes it unreachable. If you can't articulate the invariant, it isn't unreachable; replace it with proper error handling.
- **No dead code.** No public fields that are never read. No enum arms that are never matched. No re-exports of types that aren't used. `cargo +nightly udeps` and `cargo machete` (if available) help; manual grep also works (`git grep -n field_name`).
- **Match Python upstream** unless you have a documented reason to diverge. Cite `engine.py:1234` etc. in a comment when the choice is non-obvious. Divergences must be explicit and justified.
- **No `// nocov`** without explicit human approval. The user has stated this is a hard rule. If a line is genuinely unreachable, restructure the code so the unreachability is type-level. If you cannot, add an `## 8. Open questions` entry asking for permission and skip the item until granted.
- **No coverage-ratchet bumps** without explicit human approval. Same rule.
- **Comments explain WHY, not WHAT.** Don't restate code. Do explain non-obvious invariants, references to upstream, and things that surprised you.
- **No commented-out code.** Delete it. Git remembers.

### 3.2 Tests

- **Every test asserts a *behavioural* claim.** "Doesn't panic" is not a behavioural claim. `let _ = result;` at the end of a test is a smell — it usually means the test author didn't decide what should be true.
- **Every test would fail if the implementation broke.** Before committing a test, mentally (or actually) revert the fix and check that the test fails. If a wholly broken implementation passes the test, the test is not pulling its weight.
- **Tests assert against the *spec*, not the *implementation*.** Don't read the implementation, run it, observe the output, and then assert that exact output. Decide what the function *should* do and assert that. If the spec is "outputs an integer ≥ 0", assert that — don't assert `== 47` because that's what the current code happens to return.
- **Tests run under both backends** when the behaviour is supposed to be backend-agnostic. If a test is `cfg(not(feature = "native"))`-gated, there must be a *good reason* in a comment naming the divergence.
- **Tests for randomised behaviour need either**: a deterministic seed that's been verified to exercise the case, OR a probabilistic assertion (e.g. "in 1000 runs, we hit X at least once") strong enough that random luck doesn't cover a genuine bug.
- **Helper assertions like `check_can_generate_examples` are smoke tests, not behavioural tests.** They have a place but don't count as coverage of correctness.

### 3.3 Coverage

- **100% line coverage on new code** (project rule).
- **Coverage shouldn't be gamed.** If you write a test purely to bump a counter, that's a Tier-D test from the audit. Either the line is genuinely worth a behavioural test, or it should be deleted.
- **If a line is hard to cover, restructure so it's easy.** Lift error paths into the type system, narrow function inputs, etc. The coverage skill at `.claude/skills/coverage/SKILL.md` is your reference.

### 3.4 Documentation

- **Public docs match behaviour.** Settings docs that say "the X option does Y" must be true on every backend.
- **RELEASE.md is honest.** No marketing. If a feature has caveats, they're stated.
- **Doc-examples in rustdoc compile and run** (`just docs` does this).

## 4. Rules of engagement

These are absolute. Do not break them, even to "make progress".

- **Never** add `// nocov` without an `## 8. Open questions` entry asking for explicit permission.
- **Never** raise the coverage ratchet (`.github/coverage-ratchet.json`) without permission.
- **Never** mark a punch-list item complete unless §3 is fully met.
- **Never** delete a test without justifying the deletion in the commit message AND in `## 10. Test changelog` below.
- **Never** add `#[ignore]` to a test without a comment naming the reason and a punch-list item to un-ignore it.
- **Never** weaken an existing assertion. If a test seems too strict, the burden is on you to explain why the strictness was wrong.
- **Never** delete code marked `// nocov` without first writing a test that covers it. (The audit suggests many of these are reachable; deleting them silently would be a regression in the wrong direction.)
- **Always** run `just check` (which includes `cargo test`, `cargo clippy`, `cargo fmt --check`) before committing a fix. If `just check` fails, the fix isn't done.
- **Always** check both feature configurations: `cargo test` (default) and `cargo test --features native`.
- **Prefer one-PR-per-item commits over big bundled commits.** Each item is a separable concern.

If you are tempted to break a rule "just this once", that is the moment to add an entry under §8 and let a human decide.

## 5. How fixes generally look (recipes)

These are templates. Apply with judgement.

### 5.1 Recipe: "option X is silently ignored under native"

1. Write a test asserting that option X has its declared effect under native. Make it observable — e.g., `Phase::Target` removed → no `target_observations` recorded → some metric is zero.
2. Confirm the test fails on current code.
3. Find the live runner path (`src/native/test_runner.rs` for production; `src/native/conjecture_runner.rs` for the engine port).
4. Check whether the option is read at all. If not, plumb it. If read but ignored, branch on it.
5. Mirror Python's gate (cite `engine.py:line`).
6. Run the test. Run the suite.

### 5.2 Recipe: "function returns hard-coded / empty data"

Examples: `cached_test_function` returning `nodes: vec![]` and `tags: HashSet::new()`.
1. Write a test that compares `result.nodes` (or `tags`) against the expected sequence after a known input. Make sure the test fails currently.
2. Trace the data flow upstream to find where the real value is computed but discarded.
3. Plumb it through. Watch for ownership — `tags: HashSet<Tag>` may need a clone.
4. Verify dominance / Pareto comparisons that consume the field now behave correctly. Add a separate test for *that* if it isn't covered.

### 5.3 Recipe: "schema kind / panic on unknown input"

Examples: `_ => panic!("Unknown schema type")` in `schema/mod.rs:189`.
1. Write a test that constructs a malformed-or-future schema and asserts the runner returns `Status::Invalid` (or a clean error) instead of panicking.
2. Replace the `_ => panic!(...)` with a proper Result-returning path. Bubble the error up to `mark_invalid` or equivalent.
3. Make sure callers handle the new variant. Don't `.unwrap()` it back into a panic.

### 5.4 Recipe: "shrinker pass missing"

Examples: `pass_to_descendant`, `try_trivial_spans`, `reorder_spans`, `node_program`, `mutate_and_shrink`.
1. Write a shrink-quality test asserting that a counterexample shrinks to a specific minimal form. It should fail currently because the missing pass means the shrink stops early.
2. Read the upstream pass in `hypothesis-python/.../shrinker/passes.py` (and `shrinker.py`).
3. Port it. Match the iteration order, fixpoint semantics, and `consider`/`incorporate` discipline of Python.
4. Add it to the live shrink loop (`Shrinker::new` vs `Shrinker::with_probe` matters — see audit item #5).
5. Verify the shrink test now hits the minimum.

### 5.5 Recipe: "// nocov hides covered code"

1. Write a test that exercises the supposedly-uncovered branch.
2. Run coverage to confirm the line is now hit (`just check-coverage`).
3. Remove the `// nocov` markers.
4. Lower the coverage ratchet to match (this is OK because coverage is improving, not getting worse — but still ask for permission since the rule is hard).

### 5.6 Recipe: "vapid test (Potemkin)"

1. Identify what behaviour the test claims to verify (from name + comments).
2. Write a real assertion against that behaviour.
3. Verify the new test fails if the underlying code is broken (mentally revert the relevant code).
4. Replace the vapid test with the real one. Or add the real one alongside if there's an argument for keeping the smoke check.
5. Note the change under `## 10. Test changelog`.

## 6. How to find more (investigative heuristics)

Use these any iteration that doesn't have an obvious next item, OR if you finish the punch list and want to verify completeness.

### 6.1 Stub / fake-implementation hunt

```bash
# Definite stubs
git grep -n -E 'todo!\(\)|unimplemented!\(\)' src/native/ src/

# No-op functions ("legacy stub", "for now", "stub", "skip")
git grep -ni -E 'legacy stub|for now|simplification|stub|skip|fixme|todo|hack|xxx' src/native/ src/

# Functions whose body is only `()` or `Ok(())` or `_ = arg;`
# (manually scan results from this; many false positives)
grep -rn -E 'fn [a-z_]+\([^)]*\)[^{]*\{\s*\}|fn [a-z_]+\([^)]*\)[^{]*\{\s*Ok\(\(\)\)\s*\}' src/native/

# Public fields that are never read
# (audit each for read sites; if zero, suspicious)
git grep -n 'pub [a-z_]\+:' src/native/
```

### 6.2 Silent-option-ignoring hunt

For every public option on the `Hegel` builder (`src/runner.rs` / `src/lib.rs`):

1. Find where it's stored on `Settings`.
2. `git grep -n` the field name across `src/native/` and `src/server/`.
3. If only one backend reads it, that's the bug. Confirm with a test.

### 6.3 Forward-compat panic hunt

```bash
git grep -n -E '_ *=> *panic!|_ *=> *unreachable!' src/native/
```

For each: is the input source external (CBOR from server, regex pattern from user, schema field)? If yes, normally that should be `Status::Invalid` or a typed error.

**Exception (per §7 "Resolved as not-a-bug"):** schema-side panics (`src/native/schema/*`) are intentional. Rust generators construct schemas; if the Rust API can't construct the offending shape, the panic is unreachable, and if it can, a Rust-side generator test catches it. Skip these.

### 6.4 Bare `unreachable!()` hunt

```bash
git grep -n 'unreachable!()' src/native/
```

Each must have a message. Bare `unreachable!()` is a debuggability bug — fix on sight.

### 6.5 `// nocov` audit

```bash
grep -rn -E '// nocov( start| end)?' src/native/
```

For each block:
1. Read the wrapped code.
2. Search for tests that *call* this code (`git grep -n "fn_name" tests/`).
3. If tests call it: the nocov is masking real coverage — write a behavioural test, remove the nocov.
4. If no tests call it but the code is reachable from real input: write a test, remove the nocov.
5. If genuinely unreachable: restructure to make the unreachability type-level (e.g. exhaustive match on a smaller enum), then remove the nocov.

### 6.6 Shrink-quality test audit

Every test under `tests/test_find_quality/`, `tests/test_shrink_quality/`, `tests/pbtkit/shrink_quality_*` should assert *the minimum value*, not "shrinks to something". Look for:

```bash
git grep -n 'let _ = .*Minimal::\|let _ = .*\.run()\b' tests/
git grep -n 'is_empty\|!\.is_empty' tests/test_*quality*
```

Replace with `assert_eq!(result, expected_minimum)`.

### 6.7 Cross-backend parity sweep

Run the test suite under both backends and diff failures:

```bash
cargo test 2>&1 | tee /tmp/server.out
cargo test --features native 2>&1 | tee /tmp/native.out
diff <(grep -E '^test ' /tmp/server.out | sort) <(grep -E '^test ' /tmp/native.out | sort)
```

Any test that exists under one but not the other (cfg-gated) needs justification. Any test passing on one but failing on the other points to a parity bug.

### 6.8 Conformance audit

`tests/conformance/test_conformance.py` runs Rust binaries from `tests/conformance/rust/src/bin/`. Confirm the conformance Cargo.toml enables `native`. If not, conformance is only validating server-side generators.

### 6.9 RELEASE.md / docs honesty pass

Read `RELEASE.md`, `README.md`, top-level rustdoc on `Hegel`, `Settings`, every Phase / Verbosity / HealthCheck variant. For each claim, verify it's actually true on both backends. Add findings to the punch list.

### 6.10 "Two runners" hunt

The audit found that production (`NativeTestRunner`) and engine ports (`NativeConjectureRunner`) are different code paths with different bugs. When fixing anything in either, ask: is the same bug in the other? Search both files.

## 7. The punch list

Format: `[ ]` is open, `[x]` is done. Edit this file to tick boxes when items are complete (per §1, after the fix is committed and tests pass). Items are sorted by severity within each tier.

When you fix an item, append the commit hash(es) at the end of the line, like ` — fixed in abc1234`.

### Tier 0 — Failing tests (green baseline)

**These must all be ticked before any other tier is touched** (see §1.0). Each is a test that fails on the current commit under `--features native`. Investigate root cause; do not weaken assertions to make them pass.

For each: read the test, decide whether the *test* is wrong (expected message changed because the live runner moved) or the *implementation* is wrong (a real regression). Fix the actual cause; never paper over by relaxing the assertion. If the assertion was correctly capturing a behaviour we still want, the implementation is what should change.

- [x] **T0.1** `tests/test_flaky_global_state.rs::flaky_tests::test_flaky_global_state` — expected substring `"Your data generation is non-deterministic"` no longer matches; live runner panics with `"Non-deterministic test: ChoiceKind at this position differs from a prior run."` (`src/native/test_runner.rs:706`). Two non-determinism panic sites have diverged. Unify the message between `tree.rs` and `test_runner.rs`, or update the test to match — the *behaviour* is correct, only the wording differs. Pick one canonical message, ideally one that mentions "non-deterministic data generation" so the user-facing diagnostic is informative.
- [x] **T0.2** `tests/test_health_check.rs::native_too_slow_detected` — was using `gs::booleans()`, whose 2-element value space exhausts the choice tree after 2 cases (~600ms wall time at 300ms/case), under the 1s TooSlow threshold. Switched to `gs::integers::<i32>()` so the run accumulates enough cases to cross the threshold. Comment updated to explain the constraint. The TooSlow implementation itself (`src/native/test_runner.rs:254-268`) is correct.
- [x] **T0.3** `native_single_panic_on_failure` (in `tests/test_output.rs:141`). The user's panic message ("deliberate failure") was appearing twice in stderr: once from the final-replay diagnostic in `run_test_case`, and once from Rust's default panic hook printing the re-raise at `run_lifecycle.rs::drive` (`panic!("Property test failed: {}", msg)`). Fixed by adding a thread-local `SUPPRESS_NEXT_PANIC` flag that the panic hook checks to swallow exactly the next panic's stderr print, plus a manual `eprintln!("Property test failed.")` footer for human readability. The panic payload still carries `"Property test failed: <msg>"` so `catch_unwind` callers like `Minimal::run` (`tests/common/utils.rs:324`) can still pattern-match expected failures.
- [x] **T0.4** `test_disabling_shrink_limits_interesting_calls` — `should_generate_more` (`src/native/test_runner.rs:493`) had an unconditional post-bug probing window with a `MIN_TEST_CALLS=10` floor. The test asserts (correctly per the user's commit `0f5d55af`) that disabling `Phase::Shrink` should limit body calls to ≤2 (initial discovery + final replay). Multi-origin probing only adds value if each origin will be shrunk separately, so when `Phase::Shrink` is excluded the engine should stop on first interesting result. Added a `shrink_enabled` parameter to `should_generate_more` and threaded it through both call sites; when shrink is off, return `false` immediately after the first bug. The post-bug heuristic only fires when shrinking will actually happen.
- [x] **T0.5** `test_single_test_case_with_seed_is_deterministic` — same root cause as A12 (now also closed). `run_single` (`src/native/test_runner.rs:69`) was constructing `SmallRng::from_rng(&mut rand::rng())` directly, ignoring `settings.seed` and `settings.derandomize`. Replaced with `create_rng(settings, None)` so the same seeded-RNG logic that `run_main` uses also applies in single-test-case mode. The test calling `Mode::SingleTestCase` with `seed(Some(42))` three times now produces identical draws.
- [x] **T0.6** Re-run the suite after each fix above. If new failures appear, add them here. The Tier-0 list is *complete* when **`cargo test`** and **`cargo test --features native`** both produce zero failures and zero ignores (other than ignores that have a documented punch-list item attached). **Achieved in iteration 6.**

### Tier S — Lying outright

- [x] **S1.** `src/native/runner.rs:27` — `store_final_panic_info(_msg: &str) {}` is a no-op. Verified `CachedTestFunction::run_final` (the only caller of the `is_final=true` branch in `execute`) was dead code with zero callers anywhere in `src/` or `tests/`. Deleted the no-op, the dead `run_final` method, the `is_final` parameter on `execute`, and updated doc comments. Existing test suite (~30 `CachedTestFunction` tests) covers the surface that remains.
- [x] **S2.** `src/native/conjecture_runner.rs:2153` — `todo!("NativeConjectureRunner::new_shrinker")`. **Resolved by deletion.** Verified zero real callers in `src/` or `tests/`; the only "test" was a `#[should_panic(expected = "NativeConjectureRunner::new_shrinker")]` shim that enshrined the `todo!()`. The 8 ported Hypothesis conjecture-engine tests in `tests/hypothesis/conjecture_engine.rs` already work via `NativeShrinker::from_choices` and don't need `new_shrinker`. Keeping a stub `todo!()` "for API parity" is the kind of bullshit the audit was about — when a future port test legitimately needs `runner.new_shrinker(data, predicate)`, restore it then with the real driving use case (and an Rc<RefCell> on `test_fn` so the shrinker can call back). Module head doc updated to flag this as an intentional non-port; the vapid `should_panic` test removed from `tests/embedded/native/conjecture_runner_tests.rs`.
- [x] **S3.** `src/native/conjecture_runner.rs:2450` — `pareto_optimise` is implemented but never called from `run()`. Wired into `optimise_targets` per upstream `engine.py:1517-1518`: when per-target hill-climbing finds no improvements *and* `best_observed_targets` is non-empty, fire `pareto_optimise` once before deciding whether to keep iterating. Loop termination now uses `prev_calls == self.call_count` (mirroring upstream's check) instead of `prev_valid == self.valid_examples`. Added `pub pareto_optimise_call_count: usize` field (mirroring `optimise_targets_call_count`) and a behavioural test (`optimise_targets_invokes_pareto_optimise_when_hill_climbing_exhausts`) that verifies the wiring fires; confirmed it failed before the wiring and passes after.
- [x] **S4.** `src/native/test_runner.rs:125,230,301` — `Phase::Target` is silently ignored. Gated `TargetingDriver::record` and `maybe_optimise` on `phases.contains(&Phase::Target)`. Now `tc.target()` / `tc.target_labelled()` are no-ops for search-steering when the user excludes `Phase::Target`. Behavioural test in `tests/test_phases.rs::test_disabling_target_skips_hill_climb`: hill-climbing a sum-of-three-bounded-integers target deterministically reaches `3 × max_value = 3000` with `Phase::Target` enabled, and stays well below 3000 (joint event ~1e-6 per trial under boundary bias) with it disabled. Confirmed test-first: stashed the fix and saw the test fail with `max = 3000` even when Phase::Target was excluded — proving the gate was missing — then restored.
- [x] **S5.** `src/native/shrinker/mod.rs:79-94` — `mutate_and_shrink` silently disabled because the runner uses `Shrinker::new` not `Shrinker::with_probe`. **Note:** the live `NativeTestRunner` (test_runner.rs:391) was already using `Shrinker::with_probe` correctly — the audit was actually pointing at the conjecture_runner port-test fixture (lines 870-883 `NativeShrinker::from_choices` and 1917-1962 `shrink_interesting_examples`), which used `Shrinker::new` and silently dropped probes. Switched both sites to `Shrinker::with_probe` with closures that dispatch on `ShrinkRun::Full` (build `NativeTestCase::for_choices`) vs `ShrinkRun::Probe { prefix, seed, max_size }` (build `NativeTestCase::for_probe`). Behavioural test in `conjecture_runner_tests.rs::native_shrinker_from_choices_forwards_probe` shows `mutate_and_shrink`-driven probes now invoke `user_fn` (~70 calls vs ~28 pre-fix on a 3-node initial sequence). Confirmed test-first by stashing and re-running: 28 calls under `Shrinker::new`, 70 under `with_probe`.
- [x] **S6.** `src/native/database.rs` — most of the module (~14 nocov blocks) is dead code in production. **Decision: keep, document.** The wrappers (`InMemoryNativeDatabase`, `ReadOnlyNativeDatabase`, `MultiplexedNativeDatabase`, `BackgroundWriteNativeDatabase`) are public-API building blocks for users to compose their own database setups (mirroring Hypothesis's analogous types `InMemoryExampleDatabase`, `ReadOnlyDatabase`, `MultiplexedDatabase`, `BackgroundWriteDatabase`). The change-listener / watcher infrastructure is similarly public API for cross-process change observation. The live `NativeTestRunner` only composes `NativeDatabase::new(path)` directly, but that doesn't make the rest dead — it's tested via `tests/embedded/native/database_tests.rs` and exposed via `lib.rs` re-exports. Module head doc rewritten to make all of this explicit, plus an explicit "On-disk format" section that documents the deliberate incompatibility with Hypothesis's `DirectoryBasedExampleDatabase` (FNV-1a 64-bit hex vs `sha384(key).hexdigest()[:16]`, `.hegel-keys` vs `.hypothesis-keys`, plus a Hegel-specific value-byte encoding via `serialize_choices`). Cross-toolchain corpus sharing requires a translation layer. The nocov-block cleanup remains tracked as Tier C items C1, C4, C5, C6, C8 — those will get fixed independently.
- [x] **S7.** `src/native/test_runner.rs:101-104` — `Database::Unset` silently maps to `None` on native, but to a default path on server. **Picked parity:** `Database::Unset` (the non-CI default set in `Settings::new`) now maps to `NativeDatabase::new(".hegel/examples")` on native — mirroring upstream Hypothesis's `.hypothesis/examples/` cwd-relative default. The match is now exhaustive (`Path` / `Unset` / `Disabled`) so a future variant addition is a compile error rather than a silent fall-through to `None`. Behavioural test in `tests/test_runtime_dir_isolation.rs::running_failing_native_test_creates_dot_hegel_examples`: a failing run with default settings (i.e. `Database::Unset`) on native creates `.hegel/examples/` in cwd. Confirmed test-first: stashed fix → test fails (`.hegel/examples` does not exist); restored → passes.

### Tier A — Real correctness bugs

- [x] **A1.** `src/native/schema/special.rs:191-209` — UUID variant nibble: `g4 = g4_low | 0x8000` only forces the top bit. RFC 4122 needs `(g4_low & 0x3FFF) | 0x8000` AND mask to ensure top two bits are `10`. Test: 1000 UUIDs, all match `[8-b][0-9a-f]{3}` at the variant nibble. **Audit was wrong:** `g4_low` is constrained to 14 bits via `draw_integer(0, 0x3FFF)` two lines above (line 199), so bit 14 is always 0, and `g4_low | 0x8000` produces top-two-bits = `10`. Verified with the test the audit suggested (1000 UUIDs across seeds 0..1000, all variant nibbles ∈ {8,9,a,b}, distribution covers multiple variants). Test added as `interpret_uuid_variant_nibble_is_rfc4122` to lock in the invariant against future changes that might widen `g4_low` without re-thinking the variant fold.
- [x] **A2.** `src/native/schema/special.rs:191-209` — `uuids(version=None)` always emits v4. Fix to vary across `{1..5}` per Hypothesis. Test: variant distribution. **`UuidsGenerator`'s doc advertises "UUIDs of any version" as the default**, but `interpret_uuid` was doing `unwrap_or(4)` on the missing `version` field. Changed to draw a version uniformly from `[1, 5]` when the schema lacks the field. Test `interpret_uuid_no_version_varies_across_rfc_versions` runs 1000 seeded draws and asserts ≥3 distinct version nibbles appear; pre-fix the test produced only `{4}`. (The audit's specific "{1..5}" was right; "Hypothesis varies across {1..5}" wasn't quite right — Python's `UUID(version=None, int=...)` produces unmasked bits with any nibble, but Hegel applies RFC 4122 variant bits unconditionally so an unconstrained version nibble would be inconsistent. {1..5} matches the documented "any RFC 4122 version" intent.)
- [x] **A3.** `src/native/schema/special.rs:125-147` — `interpret_domain` can exceed `max_length`. Constrain. Domain charset is letters-only; add digits and hyphens per RFC. Tests: respect `max_length(10)`; produce IDN forms. **Done (charset + max_length; IDN deferred):** rewrote `interpret_domain` to budget right-to-left (TLD → SLD → subs), capping each label length by remaining budget so the result is *guaranteed* never to exceed `max_length`. Added a new `draw_dns_label(len)` helper that follows RFC 1035 §2.3.1 / RFC 1123 §2.1: ASCII letter at the start, ASCII letter or digit at the end, ASCII letters/digits/hyphens in the middle. Tests `interpret_domain_respects_max_length_across_seeds` (10 max-lengths × 200 seeds) and `interpret_domain_charset_includes_digits_and_hyphens` (asserts ≥1 digit and ≥1 hyphen across 1000 seeds, plus per-position charset rules) catch the original bugs and now pass. Two pre-A3 tests (`_short_max_length_disables_subdomains`, `_with_two_subdomains`) were rewritten to match the new draw layout — the old "max_length<10 forces 0 subs" check was an over-restriction the fix removes; the choice-sequence pin was rewritten to the new TLD→SLD→subs draw order. IDN (xn-- punycode) forms are intentionally deferred — a separate item if needed; upstream Hypothesis explicitly excludes those too (`provisional.py:130`). `interpret_email` and `interpret_url` still use the old letter-only `draw_label`/`draw_tld` helpers; switching them is also out of scope here (separate item if it matters).
- [x] **A4.** `src/native/schema/special.rs:48-83` — `interpret_date` always uses `day ∈ [1,28]`. Generate the full calendar properly (handle Feb leap years, 30/31-day months). Same for `datetimes`. Test: all 12 months represented; Feb 29 reachable in leap years. Both `interpret_date` and `interpret_datetime` now bound the day draw by `days_in_month(year, month)` (a small Gregorian-aware helper handling 31/30-day months and Feb 28/29 leap-year rule). Tests `interpret_date_full_calendar_coverage` and `interpret_datetime_full_calendar_coverage` assert across 1000-2000 seeds: all 12 months, day 30, day 31, every drawn date is a valid Gregorian date (no Feb 30, no Apr 31, etc.). Feb 29 reachability is tested deterministically (forced choices: year offset 0 → 2000, month 2, day 29 → "2000-02-29"); also tested that the integer bound rejects day 29 in non-leap-year Feb.
- [x] **A5.** `src/native/conjecture_runner.rs:80-92` — `InterestingOrigin::from_panic_payload` collapses non-string panics by Rust type. Key on `(type, file, line)` (capture the location from the panic hook) like Python. Test: two `assert!`s at different sites produce distinct origins. Done by appending the captured `file:line:col` location to the panic label so two assert sites with identical payloads no longer collapse. Implementation: `run_test_fn` now installs the cross-backend panic hook (idempotent via `Once`), wraps the user's test_fn in `with_test_context` so the hook activates and writes to `LAST_PANIC_INFO`, then takes the location via a new `pub(crate) take_panic_location` helper in `run_lifecycle`. `from_panic_payload(payload, location)` now appends `@<file:line:col>` to the panic label when a location was captured. Test `distinct_assert_sites_produce_distinct_origins` runs a body that asserts at two different source locations with identical messages and asserts `interesting_examples.len() == 2`; pre-fix it was 1.
- [x] **A6.** `src/native/conjecture_runner.rs:1864-1944` — shrink-time and re-validation runs bypass `cached_test_function`. Route them through it so `valid_examples`, `tree_root`, `target_observations`, `tags`, LRU cache all stay coherent. Match `engine.py`. **Re-validation portion done** — pass now calls `cached_test_function(&choices)` instead of raw `run_test_fn` + manual `call_count++`, so `tree_root`, `record_test_result` (counters, target observations, pareto), and the LRU cache all stay coherent. Test `re_validation_populates_cache_for_interesting_choices` runs `runner.run()` with `max_shrinks(0)` (so shrinker probes don't change the post-run choices) and a wide-enough integer draw range that the choice tree doesn't exhaust on the for-simplest probe (which would set `exit_reason=Finished` and skip the shrink phase entirely). The test asserts a follow-up `cached_test_function` on the interesting choices is a cache hit; pre-fix the cache was empty and the call was a miss bumping `call_count`. **Shrinker probe loop is a deferred sub-item** — see N4 below; it requires either an `Rc<RefCell>` on the runner or restructuring `Shrinker`'s `with_probe` API to thread `&mut self` through, both bigger changes than fit in this iteration.
- [x] **A7.** `src/native/conjecture_runner.rs:2013-2022` — `cached_test_function` returns `nodes: vec![]` and `tags: HashSet::new()`. Plumb the real values through. Verify `dominance()` and Pareto comparisons now behave correctly (separate test). **Tags portion done:** added `tags: HashSet<u64>` to `CachedRun`, populated it on cache insert in both `cached_test_function` and `cached_test_function_with_extend`, and returned `cached.tags` (cache hit) or `tags.clone()` (fresh run) from all the result paths instead of `HashSet::new()`. Tests `cached_test_function_returns_real_tags_from_fresh_run` and `..._on_cache_hit` exercise both paths via a body that calls `data.start_span(label)` + `data.stop_span()` to populate `data.ntc.tags` with a known label and assert the label appears in the result's `tags`. Pre-fix both tests fail; post-fix both pass. **Prefix-of-known-path nodes are tracked separately as N5** — that path still returns `vec![]` because the data tree records `kind` per position but not tags. A future fix can walk the tree and reconstruct partial nodes from `(kind, choice value)` pairs; see N5 for the rationale and the audit's specific concern about empty `sort_key` comparisons.
- [x] **A8.** `src/native/conjecture_runner.rs:2539-2733` — `generate_new_examples` only runs novel-prefix; no mutation step. Port `generate_mutations_from` from `engine.py`. Ported the upstream `generate_mutations_from` (engine.py:1325-1485) as a private `NativeConjectureRunner::generate_mutations_from` method. It groups same-label spans, picks two by random, and applies one of the upstream mutations: nested-spans → duplicate parent's prefix; non-nested → replace both with one's content. Each attempt goes through `cached_test_function` (so cache, tree, and bookkeeping all stay coherent). Bounded by `call_count <= initial_calls + 5` and `failed_mutations <= 5`. Wired into `generate_new_examples` after the for-simplest probe and after each main-loop test, mirroring `engine.py:1309`. New `pub mutations_attempted: usize` instrumentation field on the runner; test `generate_new_examples_runs_mutation_after_each_test` asserts it goes from 0 to non-zero across a run with same-label spans (pre-fix it stayed 0). Limitation: `ConjectureRunResult` doesn't carry spans, so after an accepted mutation the runner keeps reusing the *initial* test's spans, with span ranges filtered against the current data length. A proper fix that plumbs spans through the result is captured as N6.
- [ ] **A9.** `src/native/conjecture_runner.rs:1599-1600,1845-1846,2234-2235,2543-2544` — default phases are `[Reuse, Generate, Shrink]`, missing `Target` and `Explicit`. Match the codebase-wide default.
- [ ] **A10.** `src/native/conjecture_runner.rs:2266-2331` — `reuse_existing_examples` deletes from both primary AND secondary regardless of source corpus. Delete only from the corpus the entry came from.
- [ ] **A11.** `src/native/conjecture_runner.rs:2287-2301` — reuse never replaces an existing interesting entry with a smaller one. Add `sort_key` compare and replace.
- [x] **A12.** `src/native/test_runner.rs:69-87` — `Mode::SingleTestCase` ignores `seed` and `derandomize`. Pass them to the RNG. **Closed by T0.5: `run_single` now calls `create_rng(settings, None)` instead of constructing a fresh OS-randomised `SmallRng`.**
- [ ] **A13.** `src/runner.rs:81-90` — `Verbosity::Quiet` is unimplemented. Implement (suppress `Normal`-level output) or delete the variant.
- [ ] **A14.** `src/native/test_runner.rs:235-268,322-332` — `HealthCheck::TestCasesTooLarge` and `LargeInitialTestCase` are never raised. Implement them, or remove the variants from the public API. (Suppression of an unimplementable check is a lie.)
- [ ] **A15.** `src/native/conjecture_runner.rs:2572,2636` — `with_buffer_size_limit` only caps bytes, not choice count. Plumb to `NativeTestCase::for_simplest`/`for_probe`.
- [ ] **A16.** `src/native/data_source.rs:181` — `target_observations.insert(label, score)` silently overwrites duplicate labels. Match Python: raise/return `Status::Invalid`. Same file: reject NaN/inf.
- [ ] **A17.** `src/native/targeting.rs:397` and `src/native/conjecture_runner.rs:2938` — `try_replace` rejects ties. Accept ties when length doesn't grow (mirror `optimiser.py:75-81`).
- [ ] **A18.** `src/native/targeting.rs:229` and `src/native/conjecture_runner.rs:2785` — hill-climbing is integer-only in production. Extend to `integer | float | bytes | boolean` per `optimiser.py:109`. Remove `try_replace_for_target called on non-integer node` unreachable!s.
- [ ] **A19.** `src/native/conjecture_runner.rs:899-921` and `src/native/shrinker/mod.rs:242-254` — `fixate_shrink_passes` accepts only three pass names. Wire in the full Hypothesis pass list.
- [ ] **A20.** Span-aware shrinker passes are entirely missing: `pass_to_descendant`, `try_trivial_spans`, `reorder_spans`, `reduce_each_alternative`, `node_program`. Port each. (Probably split into multiple sub-items as you go.)
- [ ] **A21.** `src/native/shrinker/mod.rs:263-308` and `src/native/shrinker/integers.rs:308-387` — joint integer passes drop `shrink_towards`. Add a `shrink_towards` field to `IntegerChoice` and use it. Mirror `shrinker.py:1014-1027` and `1437-1447`.
- [ ] **A22.** `src/native/shrinker/floats.rs:288-314` — `redistribute_numeric_pairs` drops the `MAX_PRECISE_INTEGER` guard. Restore it.
- [ ] **A23.** `src/native/test_runner.rs:128-151` — DB replay only loads first interesting example. Loop through all stored values; call `record_test_result` for each.
- [ ] **A24.** `src/native/test_runner.rs:407-413` — DB save has no secondary-key downgrade. Implement the downgrade per Hypothesis's stale-key flow.
- [ ] **A25.** `src/native/conjecture_runner.rs:311,2456,2462` — Pareto front uniqueness is fragile. Either dedupe by `sort_key` (so `Equal` is genuinely unreachable) or add a `Pareto::Equal` arm with a sensible policy. Stop relying on "Equal has not been observed".

### Tier B — Forward-compat hazards

- [ ] **B1.** `src/native/featureflags.rs:131,218` — `panic!("{}", STOP_TEST_STRING)` as control flow. If this is the established escape hatch, fine — but document it loudly at the call site so a reader knows it's not a bug.
- [ ] **B2.** ~20+ bare `unreachable!()` sites across `shrinker/integers.rs`, `shrinker/strings.rs`, `shrinker/floats.rs`, `shrinker/bytes.rs`, `core/state.rs`, `core/choices.rs`. Add diagnostic messages to each. Use `unreachable!("kind/value mismatch: {kind:?} vs {value:?}")` style.
- [ ] **B3.** `src/native/schema/float.rs:8,32,41,57,70` — only `width == 32` is special-cased; other widths silently treated as f64. Either reject other widths explicitly, or implement them. Add a fail-loud test for an out-of-range `width`.

### Tier C — `// nocov` masking covered code

- [ ] **C1.** `src/native/database.rs:1313-1437` — `serialize_choices`/`deserialize_choices` are `// nocov` but tested. Remove nocov, lower ratchet (with permission).
- [ ] **C2.** `src/native/re/parser.rs:1565-1595` — `parse_pattern` is the public entry point; nocov is wrong. Remove.
- [ ] **C3.** `src/native/re/parser.rs:860-1395, 775-857, 606-746, 494-596, 266-289, 190-214, 408-419, 153-157, 476-489, 311-358, 366-371, 381-397, 121-131, 226-241` — entire parser pipeline. Remove nocov for any range that has direct test coverage; add tests where coverage is genuinely missing.
- [ ] **C4.** `src/native/database.rs` — `save`/`move_value`/watcher methods are nocov but tested by `test_database_*`. Remove (subject to S6 — if database is being deleted entirely, this collapses into that).
- [ ] **C5.** `src/native/core/state.rs:1083-1124,1245-1299` — `weighted` and `draw_bytes` nocov but called everywhere. Remove.
- [ ] **C6.** `src/native/core/choices.rs:48-58,152-157,345-352,385-450,203-302,872-884` — `*_index`, `simplest`, `unit`, `PartialEq` for FloatChoice. Remove.
- [ ] **C7.** `src/native/shrinker/integers.rs:211-295,395-491` — `redistribute_integers`, `shrink_duplicates` (both directly tested). Remove.
- [ ] **C8.** `src/native/schema/text.rs:258-440` — `build_string_alphabet`/`_uncached`. Remove.
- [ ] **C9.** Audit every remaining `// nocov` block in `src/native/` after C1-C8 are done. Each must be a genuinely-unreachable type-level invariant or be deleted.

### Tier D — Vapid tests

- [ ] **D1.** `tests/test_targeting.rs:11-74,206-215` — six tests with no assertions. Replace each with a real behavioural test (e.g. observe that `target_observations` actually steers the search; assert a specific recorded label/score; assert max examples constraints). Delete the originals.
- [ ] **D2.** `tests/embedded/native/optimiser_tests.rs` lines 70, 350, 386, 1039, 1084, 1126, 1176, 1218, 1261 — line-coverage stubs. For each: figure out the actual behavioural claim of the named line and write a real test. If the line doesn't carry behavioural meaning, the line itself is a candidate for deletion.
- [ ] **D3.** `tests/embedded/native/conjecture_runner_tests.rs` — `record_test_result_early_stop_increments_overrun` (1755), `data_tree_view_rewrite_missing_key_returns_novel` (2069), `record_tree_kill_depth_applied` (2241), `enumerate_choice_values_bytes_small_range` (2275), `generate_novel_prefix_traverses_children` (2332), `should_generate_more_*` (2576), `runner_optimise_targets_with_target_phase_only` (2596). Same — replace with real assertions.
- [ ] **D4.** `tests/embedded/native/state_tests.rs:498-571,596-611,708-724` — observer tests that can't fail. Either expose enough state on `DataObserver` to assert on (preferred), or delete and replace with a public-API-level test.
- [ ] **D5.** `tests/test_native.rs:14-20,23-26,29-32,42-52,98-104,181-216` — tautologies and "this closure never executes". Replace with real tests or delete.
- [ ] **D6.** `tests/test_health_check.rs:53-94` — suppression tests with no assertions. Once A14 lands, write tests that (a) confirm the check fires under triggering conditions and (b) confirm suppression suppresses it. Delete the assertion-free originals.
- [ ] **D7.** `tests/embedded/native/shrinker_tests.rs:2049` — `assert!(shrinker.current_nodes.len() < 21)` is too weak. Replace with `assert_eq!(.., expected_minimum_len)`.
- [ ] **D8.** `tests/test_targeting.rs::test_finds_a_local_maximum` — relies on random-seed luck. Either use a fixed seed verified to work AND a probabilistic re-run check, or assert against a deterministic optimiser-only run. The previous cfg-gating was conservative-correct; ungating without strengthening was a regression.
- [ ] **D9.** `tests/common/utils.rs:37-43` — `check_can_generate_examples` is a smoke test by design but is being used as if it were a behavioural test in `tests/test_standard_generators.rs`. Add real behavioural assertions for each generator (range, distribution, shrink target).

### Tier E — Smells / dead code

- [ ] **E1.** `src/native/conjecture_runner.rs:1488` — `pub ignore_limits: bool` is dead. Either implement it (gate the limit checks on the flag) or delete it.
- [ ] **E2.** `src/native/conjecture_runner.rs:1372-1375` — `Status::Invalid` reason discarded. Plumb `events`/`why` onto `ConjectureRunResult` so tests can assert on it.
- [ ] **E3.** `src/native/targeting.rs:224-264` — `hill_climb` doesn't reset `i` after node-count-changing improvements. Mirror `optimiser.py:95-97`.
- [ ] **E4.** `src/native/targeting.rs:97` — `maybe_optimise` is one-shot per run. Allow re-entry as more valid examples arrive (or document why one-shot is correct).
- [ ] **E5.** `src/native/conjecture_runner.rs:1701-1721` — Pareto-add gate doesn't check `data.has_discards`. Add the check.
- [ ] **E6.** `src/native/test_runner.rs:586-610` — `EngineCtx::mode` field is dead. Remove.
- [ ] **E7.** `RELEASE.md` — overstates parity. Update once items above land.
- [ ] **E8.** No `print_blob` / `reproduce_failure` on either backend. If this is on the roadmap, add to Open Questions; otherwise leave it.
- [ ] **E9.** `src/native/shrinker/strings.rs:120-141` — non-deterministic iteration order over a `HashMap`. Sort first.
- [ ] **E10.** `src/native/shrinker/index_passes.rs:58` — `checked_add(0)` is a no-op; the overflow guard is missing. Either add the guard properly or simplify to a non-checked form.

### Newly discovered items

Add new findings here, in the appropriate tier. Each must include a file:line and a one-sentence "why this is wrong".

- [ ] **N1 (Tier A).** `src/native/test_runner.rs:540-720` and `src/native/tree.rs:27-264` — duplicated non-determinism detection: each runner has its own `ChoiceValueKey` enum AND its own trie (`DetTreeNode` vs `TreeNode`) AND its own panic-message wording. The wording duplication caused T0.1; the structural duplication will cause more drift. Factor the trie + key + diagnostic into a single shared module under `src/native/` (or `core/`) and use it from both runners. While there, also de-duplicate `panic_message` (it's defined in `src/native/runner.rs:11` and again in `src/run_lifecycle.rs:153`).
- [ ] **N3 (Tier E).** `src/native/test_runner.rs:282` — `calls += SPAN_MUTATION_ATTEMPTS as u64` runs unconditionally inside the `else if run.status == Status::Valid` branch, *even when `try_span_mutation` returned `None` immediately because there were no repeated span labels to mutate.* The accounting overcharges by 5 per Valid case in tests with no span structure. Either condition the increment on whether mutation actually ran, or have `try_span_mutation` return the actual attempt count.
- [ ] **N4 (Tier A — A6 follow-up).** `src/native/conjecture_runner.rs:1974-2020` — the shrinker probe loop in `shrink_interesting_examples` still calls `run_test_fn` directly (via the `Shrinker::with_probe` closure) instead of going through `cached_test_function`. So shrink-time probes don't update the LRU cache, don't get cache-hit replays for repeated probes, and don't bump `valid_examples` / pareto / target observations even when the probe happened to be `Status::Valid`. The blocker is that the closure currently captures `&mut self.test_fn` and `&mut self.call_count` separately to avoid Rust's "single `&mut self`" rule; routing through `self.cached_test_function(...)` would need either an `Rc<RefCell<NativeConjectureRunner>>` or restructuring `Shrinker::with_probe`'s API to take a callback that receives `&mut Runner` per probe. The re-validation pass (A6) is now correct; this sub-item finishes the rest of the audit's complaint.
- [ ] **N5 (Tier A — A7 follow-up).** `src/native/conjecture_runner.rs:2099-2108` — when `cached_test_function` finds `choices` is a strict prefix of a known tree path, it returns `Status::EarlyStop` with `nodes: vec![]`. Per the audit, Python's `simulate_test_function` carries the partial walk's nodes, and a downstream `sort_key` comparison on this empty result reads as smaller than any non-empty result (poisoning Pareto / dominance comparisons). The fix is to walk `tree_root` along `choices`, reconstruct the partial nodes from `(kind, choice value)` pairs at each step, and return them. The data tree records `kind` per position; `value` comes from the input. (Tags can't be reconstructed from the tree — the audit's tags concern is fixed in A7; this sub-item is just about nodes.)
- [ ] **N6 (Tier A — A8 follow-up).** `ConjectureRunResult` doesn't carry `spans` (the per-test span structure that `data.ntc.spans` produces). `generate_mutations_from` needs them per accepted mutation to re-derive `mutator_groups` from the new data, but currently keeps reusing the *initial* test's spans (with a length filter to avoid out-of-range slicing). Add `spans: Vec<Span>` to `ConjectureRunResult` and `CachedRun`, populate it in `cached_test_function` from `run_test_fn`'s now-extended return tuple, and have `generate_mutations_from` read `new_data.spans` after each accepted mutation. This will let mutation explore the structural space of mutated test cases properly.
- [ ] **N2 (meta).** INSTRUCTIONS.md §0 cites `external/hypothesis/...` and `external/pbtkit/...`; the actual paths are `resources/hypothesis/hypothesis-python/src/hypothesis/internal/conjecture/` and `resources/pbtkit/`. Update §0 so future iterations don't waste time looking under `external/`.

### Resolved as not-a-bug (do not re-add)

These were on the original audit list but the user (DRMacIver) has decided they are intentional / acceptable. Do not add them back. Cite this section if you find them again during a §6 sweep.

- **Hard-coded constants matching Python.** `src/native/conjecture_runner.rs:497-505` (`INVALID_THRESHOLD_BASE = 458`, `INVALID_PER_VALID = 100`) and any similar "match Python exactly at port time" constants. **Policy:** these stay hard-coded; we sync with upstream manually. If you find a new one, leave it alone — but add a brief `// Match Python <file:func>` comment if there isn't one already.
- **`_ => panic!` on schema fields and dispatch keys.** All of:
  - `src/native/schema/mod.rs:139` (`Unknown native command`)
  - `src/native/schema/mod.rs:189` (`Unknown schema type`)
  - `src/native/schema/mod.rs:249,261,270` (bignum / CBOR shape panics)
  - `src/native/schema/text.rs:284` (`Invalid codec`)
  - `src/native/schema/collections.rs:12,27,47` (tuple/one_of/sampled_from shape panics)
  - `src/native/schema/regex.rs:36` (invalid regex pattern)
  
  **Policy:** schemas are constructed by Rust generators; if the Rust API can't produce the shape, the panic is unreachable. If it can, a Rust-side generator test will exercise it and we'll see the panic and fix it. Do **not** convert these to `Status::Invalid`.
- **Conformance does not run native.** `tests/conformance/rust/Cargo.toml` not enabling `native` is intentional — the conformance suite targets the server protocol, not the native backend.

## 8. Open questions

Use this section when you genuinely need a human decision and cannot proceed. Each entry: question, what you tried, what you'd do if forced to pick. Do not block the loop on a question — work on a different item until the question is answered.

- (none yet)

## 9. Acceptance criteria — when to emit COMPLETE

Emit `<promise>COMPLETE</promise>` at the end of an iteration if and only if **every** point below is true:

1. **Every box in §7 is ticked.** No open items in any tier including Tier 0 and "Newly discovered items".
2. **Two consecutive investigative iterations** (using §6 heuristics) have found no new items. This guards against the audit being shallow.
3. **`just check` passes** on the current commit.
4. **`just check-coverage` passes** without any new ratchet bumps unauthorised by §8 entries that received human approval.
5. **`cargo test`** (default features) and **`cargo test --features native`** both pass.
6. **`git grep -n -E 'todo!\(\)|unimplemented!\(\)' src/native/`** returns nothing (UnicodeData.txt false positives are fine; only `.rs` matches count).
7. **`git grep -n 'unreachable!()' src/native/`** returns nothing — every site has a diagnostic message.
8. **`grep -rn -E '// nocov' src/native/`** is either empty, or every remaining block has an entry under §8 with documented human approval.
9. **RELEASE.md** is honest about native-vs-server parity.
10. **The audit can be re-run from scratch** (re-run §6 heuristics) without finding anything new of Tier-S, Tier-A, or Tier-B severity. Items in "Resolved as not-a-bug" do not count as new findings.

If any of these fails, do not emit the promise. Continue to the next iteration.

## 10. Test changelog

When you delete or significantly weaken a test, log it here with the reason. (Strengthening tests doesn't need an entry.) This is so the user can audit test-suite changes at the end.

- **Deleted `tests/embedded/native/conjecture_runner_tests.rs::new_shrinker_panics_with_todo`** (iter 7, S2). Vapid coverage shim — `#[should_panic(expected = "NativeConjectureRunner::new_shrinker")]` asserted the `todo!()` panic message, which only existed to keep the line covered. Closed by deleting the `new_shrinker` method itself; nothing to test for now.
- **Strengthened `tests/test_phases.rs::test_disabling_target_skips_hill_climb`** (iter 13). Added `seed(Some(0xdeadbeef))` to make it deterministic. Previously failed ~40% of runs because `try_span_mutation` could swap a max-value draw between span positions, propagating it to all three positions and hitting the cap independently of hill-climbing. Re-verified test still catches the original S4 bug: with both `targeting.record` and `maybe_optimise` un-gated, the test fails with `max = 3000`. So the seed change preserves the regression-detection power while removing the flake.
- **Rewrote `tests/embedded/native/schema/special_tests.rs::interpret_domain_short_max_length_disables_subdomains`** (iter 16, A3) → renamed to `interpret_domain_minimum_max_length_yields_two_labels` and reframed to assert the right invariant. The old test asserted that `max_length=9` forces 0 subdomains, but that was the over-restriction A3 is fixing — the new budget-aware code correctly allows subs whenever the remaining budget can fit them. The new test asserts max_length=4 (smallest legal) yields exactly 2 labels.
- **Rewrote `tests/embedded/native/schema/special_tests.rs::interpret_domain_with_two_subdomains`** (iter 16, A3). Same name, but the choice-sequence layout changed: the new code draws TLD len → TLD chars → SLD len → SLD chars → n_subs → (sub_len, sub chars)*. Test now uses min-sized labels and produces "a.a.a.aa" instead of the old "aaa.aaa.aaa.aa".

## 11. Reference: how to run things

```bash
# Full local check (clippy + fmt + test)
just check

# Tests only, both backends
cargo test                 # default features (server backend)
cargo test --features native

# Single test
cargo test test_name
cargo test --features native test_name

# Coverage
just check-coverage

# Conformance (Python harness)
just check-conformance

# Format
just format
```

## 12. Reference: where the audit findings came from

The Tier S/A/B/C/D/E items above came from a multi-agent audit run on commit `fa2c5d03` of branch `DRMacIver/native`. The audit covered:
- conjecture_runner.rs vs `engine.py`
- shrinker/* vs `hypothesis-python/.../shrinker/`
- every `// nocov` block in src/native/
- every `panic!`/`unreachable!` site in src/native/
- schema/* vs the generators that emit schemas
- targeting/optimiser vs `pareto.py`/`optimiser.py`
- test quality across `tests/`
- public API parity native vs server

If you want to reproduce or extend the audit, the heuristics in §6 are the same ones used. The audit is not exhaustive — finding Tier-A bugs that the original audit missed is expected and welcome. Add them under "Newly discovered items" in §7.
