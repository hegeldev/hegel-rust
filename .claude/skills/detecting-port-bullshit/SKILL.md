---
name: detecting-port-bullshit
description: "Audit code derived from DRMacIver/native for the systematic failure modes the porting agent fell into. Use whenever you have copied or adapted code from DRMacIver/native — before opening a PR, after pulling new files over, or when reviewing an existing extraction PR. Catalogues backdoors, silent no-ops, dead code, cargo-culting, and other patterns the agent produced reliably."
---

# Detecting bullshit in DRMacIver/native ports

The agent that produced `DRMacIver/native` was tasked with building a native (in-process) Rust backend for Hegel whose observable behaviour matches the Python/Hypothesis server backend. **It did not always do this.** A non-trivial fraction of the code subverts that goal: silent no-ops that claim to honour settings, fake stubs returning hardcoded data, parallel-but-divergent code paths, dead machinery kept "for API parity", coverage gamed via feature blackouts, and tests that pass on a broken implementation.

Your job here is to find and remove every instance of these patterns in code copied or derived from that branch, **before** the code goes in front of a human reviewer. The goal is a diff that reads as if a competent human wrote it from scratch.

This skill is invoked from `landing-native-chunk` step 4, but you can also run it standalone on any extraction PR or any commit derived from DRMacIver/native.

## How to run an audit

1. **Read the entire diff.** Not just changed files — every line of new code. Speed-reading is how bullshit hides. If the diff is too large to fully read, the chunk is too large; split.
2. **Walk every pattern in §1–§16 below in order.** Each section has a detection command and a remediation. Run the commands; don't skim.
3. **For every finding: TDD.** Write a regression test that fails on the bullshit, then fix the bullshit, then confirm the test passes. The CLAUDE.md "Aim to work in a TDD style" rule applies hard here — most bullshit only stays fixed if a test pins the correct behaviour.
4. **Re-run §1–§16 after fixing.** Every fix can introduce new bullshit.
5. **Run `just check` and `just check-coverage`** as the final gate. Both must pass without `// nocov` additions, ratchet bumps, or `#[ignore]` additions.

When in doubt, **the burden of proof is on the code, not on you**. "This looks fine but I'm not sure" → write the test that would prove it fine. If you can't articulate the correct behaviour clearly enough to write a test, you don't understand the code well enough to ship it.

---

## §1. Silent option-ignoring and fake stubs

The agent's most frequent failure: accept a public setting, return without errors, and do nothing.

**Detection:**
```bash
git grep -nE 'fn [a-z_]+\([^)]*\)[^{]*\{\s*\}' src/native/   # empty bodies
git grep -nE 'fn [a-z_]+\([^)]*\)[^{]*\{\s*Ok\(\(\)\)\s*\}' src/native/
git grep -niE 'legacy stub|for now|simplification|no-op|todo|fixme|hack|xxx' src/native/ src/
```

For every public `Settings` / `Phase` / `Verbosity` / `HealthCheck` variant: search the native dispatch path for it. If only the server path reads it, that's the bug.

**Examples from PR #262:**
- `NativeDataSource::mark_complete` was a no-op; outcome was being smuggled through a different channel, bypassing the cross-backend `DataSource` interface (fixed in `6539f862 fix(native): purge DataSource bypasses and silent no-ops`).
- `target_observation` validated its input and stored into a `target_observations` map that was never read anywhere — implementing the *appearance* of `Phase::Target` while doing nothing.
- `Phase::Target` was silently ignored in the live runner (`S4` in the late audit).
- `Verbosity::Quiet` was unimplemented (`A13`).
- `Mode::SingleTestCase` ignored `seed` and `derandomize` settings (`T0.5` / `A12`).

**Remediation:** Either honour the option on the native path with a behavioural test that asserts the *observable* effect (e.g. `Phase::Target` removed → target_observations metric is zero), or reject the option at the boundary with a clear error. **"Compiles, accepts, does nothing" is not acceptable.**

## §2. Variable laundering

The agent worked around unused-variable warnings instead of removing the unused code.

**Detection:**
```bash
git grep -nE 'let _ =' src/native/                       # explicit discard
git grep -nE 'let _[a-z]' src/native/                    # _-prefixed name
git grep -nE 'let _[a-z][a-z_]+ *=' src/native/          # _x_unused-style
```

**Examples from PR #262:** `representative_origin` was assigned in three places and consumed only by `let _ = representative_origin;`. `_target_phase_enabled = settings.phases.contains(&Phase::Target);` — set but never read. Both removed in `6539f862`.

**Remediation:** Per the user's global CLAUDE.md: "When getting an unused variable warning, do not suppress it or rename the variable with an underscore prefix." Just delete the assignment, and the expression too if appropriate. `let _ = …` is only acceptable to discard a `#[must_use]` return value where the discard is genuinely intentional (rare). Variable laundering reliably indicates the *previous* code path is dead.

## §3. Functions returning hardcoded / empty data

A function whose name promises real data but whose body returns `vec![]`, `HashSet::new()`, `0`, or `Default::default()`.

**Detection:** Grep `src/native/` for `vec![]`, `HashSet::new()`, `BTreeMap::new()` returns in non-constructor functions. Also look for `return None` / `return Ok(None)` in places that semantically should return real values.

**Examples from PR #262 / native audit:**
- `cached_test_function` returning `nodes: vec![]` and `tags: HashSet::new()` (`A7`). Dominance / Pareto comparisons that consumed these fields then made incorrect decisions.
- `Database::Unset` mapping silently to `None` on native, while the server backend mapped it to a default `.hegel/examples` path (`S7`).

**Remediation:** Trace data flow upstream — usually the real value is computed and then dropped. Plumb it through. Add a regression test that uses the field for an observable downstream effect.

## §4. Two-runners / parallel-but-divergent code paths

The agent built `NativeTestRunner` (production) and `NativeConjectureRunner` (port-test fixture) as separate code paths. Bugs in one rarely got fixed in the other.

**Detection:** Any function or file with a near-twin elsewhere in `src/native/`. The original case was `det_tree.rs` being a copy of `data_tree.rs`'s non-determinism detection — collapsed in `b2e8478b review: drop det_tree, tree.rs, native dispatch loop; pbtkit→Hypothesis`.

```bash
# Files with suspiciously similar names:
ls src/native/ src/native/**/  # look for foo.rs + foo_tree.rs, runner.rs + test_runner.rs etc.
# Functions duplicated across files:
git grep -nE 'fn (run|run_main|run_single|cached_test_function|execute)\b' src/native/
```

**Remediation:** Collapse to one. If both paths exist legitimately (one for production, one as a port fixture for embedded tests), one of them is a thin wrapper around the other — not a parallel implementation.

## §5. Single-impl traits and over-abstraction

The agent added abstractions for things that have one implementation and no second consumer in sight.

**Detection:**
```bash
git grep -nE 'pub trait [A-Z]' src/native/
# For each trait, count impl sites:
git grep -nE 'impl [A-Za-z_]+ for ' src/native/
```

**Example from PR #262:** `src/native/tree.rs` defined a `NativeRunner` trait with a single impl (`EngineCtx`) and no consumers. Collapsed into an inherent method on `EngineCtx` (`b2e8478b`).

**Remediation:** Inline single-impl traits. If a future second implementation is plausible, add the trait *then* — abstractions chosen for hypothetical extensibility almost always model the future wrong.

## §6. Cargo-culted server-protocol dispatch in in-process code

The server backend talks to the Python process via a CBOR-encoded message protocol. The native backend is in-process and does **not** need this protocol — but the agent ported the dispatch shape anyway.

**Detection:**
```bash
git grep -nE 'dispatch_request|cbor_to_|as_bool|as_u64' src/native/
```

**Example from PR #262:** `NativeDataSource` was routing `start_span` / `stop_span` / `new_collection` / `pool_*` calls through a `schema::dispatch_request` CBOR loop. Replaced with direct calls via a `with_ntc` helper (`b2e8478b`). The unused `pool_consume` command and the unused CBOR helpers (`cbor_to_i64`, `as_bool`, `as_u64`) came out with it.

**Remediation:** In-process calls go through Rust function calls. CBOR machinery in `src/native/` is almost always a sign of porting confusion — only `core/choices.rs` and the `schema/` dispatch (which receives CBOR from the user-facing Rust API) legitimately deal with CBOR values.

## §7. Dead code retained "for API parity"

Public fields nobody reads. Trait methods with defaults that no impl overrides. Re-exports of types nothing imports. Variants nothing constructs. `force_simplest`-style "kept around in case someone needs it".

**Detection:**
```bash
git grep -nE 'pub [a-z_]+:' src/native/                  # public fields
# For each, count read sites cross-tree:
git grep -n '\.field_name\b' src/ tests/

# Single-impl trait defaults:
git grep -nE 'fn [a-z_]+\([^)]*\)[^;]*\{\s*[a-zA-Z_]' src/native/  # then check if overridden

# Pub items never imported:
git grep -nE 'pub (fn|struct|enum|trait|const) ' src/native/
```

**Examples from PR #262:**
- `Span::choice_count`, `Spans::iter` — public methods nobody called.
- `NativeTestCase::force_simplest` and the `else if force_simplest` arm in `resolve_choice` — together with the field itself, all dead. Deleted in `7ca8032a`.
- Trait default `move_value` body that no impl overrode.
- `ExampleDatabase::as_any` and `db_eq` methods plus `NativeDatabase`'s `PartialEq`/`Eq` impls — zero callers.
- `DataSource::test_aborted` — became dead after `mark_complete` became the outcome channel, but kept hanging around. Kept as `#[cfg(test)]` for assertion-only use.

**Remediation:** Delete. Per the user's global CLAUDE.md: "If you are certain that something is unused, you can delete it completely." If you're not certain, `cargo +nightly udeps` and grep more carefully. Coverage-ratchet rules apply: if removing dead code lowers the ratchet, great — let the ratchet drop, don't bump it.

**Reflexive trap:** the audit ID `S2` (`new_shrinker` `todo!()` "kept for API parity") was the canonical example of this. The fix was to delete it. When you see "kept for parity with the Python class shape", read it as "delete me".

## §8. `// nocov` annotations on reachable code

`// nocov` is a sticking-plaster the agent reached for when a line was hard to cover. Most of these blocks turn out to be reachable from the public API; the agent just didn't write the test.

**Detection:**
```bash
git grep -rn 'nocov' src/native/ src/
```

For every block:
1. Read the wrapped code.
2. Grep for callers in `tests/` and `src/`. If anything calls it through real public API, the nocov is masking real coverage — write the behavioural test and remove the marker.
3. If genuinely unreachable, restructure the code so the unreachability is *type-level* (exhaustive match on a smaller enum, `unreachable!` removed by a stricter type).
4. **Do not add new `// nocov` annotations.** The user's global CLAUDE.md is explicit and the project's `.claude/CLAUDE.md` re-states: "**CRITICAL: You MUST NOT increase the numbers in `.github/coverage-ratchet.json` without first asking for and then receiving explicit human permission to do so.**" `// nocov` is the same kind of escape hatch and needs the same level of permission.

**Example from PR #262 / native audit:** `Q2` resolved two stubborn nocov markers in `database.rs` and `choices.rs` by *deleting* them and adding real behavioural tests that exercised both paths (default trait listener methods, surrogate-block fallback in `simplest_codepoint`).

## §9. Native gates: prefer xfail over silent skip

A failing test was hard to fix, so the agent gated it with `#[cfg(not(feature = "native"))]` or `#[ignore]`. The result is dormant code that no one notices when its underlying gap closes.

**Two annotations, two purposes:**

- **`#[not_supported_on_native]`** — *temporary* xfail. An attribute proc-macro from `hegeltest-macros`, re-exported through `tests/common/mod.rs`. The test still runs under `--features native`; the runner expects it to panic, so the test fails loudly the moment it starts passing. This is the right choice for any test gated on a missing engine feature.
- **`#[cfg(not(feature = "native"))]`** — *permanent* exclusion. Only correct for tests that depend on Python-only behaviour, server-only API, or some intentional divergence that will never be on native.

**Detection:**
```bash
git grep -n 'not_supported_on_native' tests/                       # temporary xfails
git grep -n '#\[cfg(not(feature = "native"))\]' tests/             # permanent + legacy
git grep -n '#!\[cfg(not(feature = "native"))\]' tests/            # file-level (almost always wrong)
git grep -n '#\[ignore' tests/ src/                                 # bare ignores
git grep -n 'cfg_attr.*feature = "native".*allow' tests/           # lint-noise suppressions
```

For every `#[ignore]`: read the message. If there's no associated bug-tracker entry or documented limitation, the ignore is hiding a real failure.

For every `#[cfg(not(feature = "native"))]`: is it (a) temporary, waiting on an engine feature — migrate to `#[not_supported_on_native]`; or (b) permanent — keep the `cfg` and write a one-line comment naming the *unrepresentable concept* (e.g. `// Native: tests Python repr() formatting; no Rust counterpart`). Anything in between is bullshit.

For every `#![cfg(not(feature = "native"))]` (file-level): almost never the right call. Split the file: temporary tests get `#[not_supported_on_native]`, permanent ones get per-item `#[cfg(...)]`.

**When landing a new chunk, the bar is higher: default to ungating.** See `landing-native-chunk` step 3.5 for the inventory-and-ungate process. The xfail behaviour does most of the work — every previously-temporary gate whose underlying feature now lands surfaces automatically as a "test did not panic as expected" failure under native, telling the next chunk's author which annotations to drop. A chunk that adds gates without removing any is suspect.

**Generic-comment audit:** existing gates with no comment, or comments like `// native doesn't support this yet`, `// TODO: native`, `// not on native`, are not actionable. Each remaining gate should name the *specific* missing engine piece (for `#[not_supported_on_native]`) or the *unrepresentable concept* (for `#[cfg(not(feature = "native"))]`).

**Lint-suppression sweep:** the `#![cfg_attr(feature = "native", allow(unused_imports, dead_code))]` blocks PR #262 added to a handful of test files exist only to silence clippy when file-level cfg gates leave imports dangling. Migrating to per-test `#[not_supported_on_native]` makes the helpers actually live under native, so the suppression stops doing anything — delete it.

## §10. Vapid / Potemkin tests

Tests that pass on a wholly broken implementation. The agent wrote many of these to bump the coverage counter.

**Detection:**
```bash
git grep -nE 'let _ = .+\.run\(\)' tests/         # Discarded run results
git grep -nE 'assert!\(.*is_empty\(\)|!.*is_empty\(\)' tests/  # Asserting "found something"
git grep -nE 'should_panic' tests/                # Sometimes used to enshrine todo!()
```

For each candidate, mentally revert the implementation (replace it with `Default::default()` or a constant) and ask: would the test still pass? If yes, the test is not pulling its weight.

**Examples from the audit:**
- "Tests for `T1` were checking that `tc.draw(…)` didn't panic" rather than asserting anything about the value.
- `should_panic` tests enshrining a `todo!()` stub (canonical example: `S2`'s `should_panic(expected = "NativeConjectureRunner::new_shrinker")` "test" — fixed by deleting both the stub and the vapid test).
- Shrink-quality tests that `assert!(result.is_some())` instead of `assert_eq!(result, Some(expected_minimum))`.

**Remediation:** Per the user's global CLAUDE.md, regression tests are mandatory for bug fixes. Rewrite vapid tests to assert against the *spec*, not against whatever the current code happens to return. For shrink-quality tests: assert the *exact minimum* the shrinker should find, not "shrinks to something".

## §11. Bare `unreachable!()` and `panic!` on external input

Two related anti-patterns:

1. `unreachable!()` with no message — pure debuggability bug. If it ever fires, you have no idea why.
2. `_ => panic!("Unknown X")` on input that crosses a backend boundary (schema parsing, CBOR decode, user-supplied regex). This crashes the test runner when the right answer is `Status::Invalid` or a typed error.

**Detection:**
```bash
git grep -n 'unreachable!()' src/native/                  # bare
git grep -nE '_ *=> *panic!|_ *=> *unreachable!' src/native/
```

**Remediation:**
- For `unreachable!()`: add a message naming the invariant. If you can't articulate the invariant, it isn't actually unreachable — replace with proper error handling.
- For `_ => panic!` on external input: return `Status::Invalid` or a typed error and bubble up to the test runner's invalid-case path. Exception: schema-side panics (`src/native/schema/*`) where the Rust API can't construct the offending shape are intentional and acceptable, *if* there's a generator test that confirms the inability.

**Example from PR #262 / native audit:** `N16` — three sites in shared generator code with bare `unreachable!()` after `tc.assume(false)`. Each got a message describing why the `unreachable!()` was actually unreachable (the `assume` panics first with `ASSUME_FAIL_STRING`).

## §12. pbtkit-as-authoritative

The native engine is meant to match **Hypothesis** semantics. pbtkit is a cleaner reference implementation of the same core ideas — useful for reading, but **not the behavioural source of truth**.

The agent's own `implementing-native` skill (in DRMacIver/native, not yet on main) gets this right: "Hypothesis is the behavioural target." But the agent reliably forgot, and citations slipped.

**Detection:**
```bash
git grep -nE 'pbtkit|Port of pbtkit' src/
```

For every hit, check whether the citation is doing one of:
- **Acceptable**: pointing at a pbtkit module as a *clearer reference implementation* of a Hypothesis primitive, alongside or in addition to the Hypothesis citation.
- **Acceptable**: pointing at a pbtkit module that has no direct Hypothesis counterpart (pbtkit is a subset; some primitives only exist there).
- **Bullshit**: citing pbtkit as the *authoritative* behavioural reference when Hypothesis has a counterpart — and especially when pbtkit and Hypothesis differ.

**Example from PR #262:** `b2e8478b review: drop det_tree, tree.rs, native dispatch loop; pbtkit→Hypothesis` swept the codebase replacing `"Port of pbtkit's X"` doc comments with `"Port of Hypothesis's X"` (or both, where appropriate). Also dropped a swathe of `pbtkit/audit-item` taxonomy terminology (`N18.run_lifecycle:`, `N8:`, `Pre-N8`).

**Remediation:** When the Rust function is doing something that exists in both Hypothesis and pbtkit, cite Hypothesis and (optionally) note "pbtkit's `X.py` is a more readable version of the same idea." When they disagree, the Rust code must match Hypothesis; say so in the comment.

## §13. Database / format compatibility hazards

The native backend persists state to disk. Anything that touches that on-disk format risks colliding with Hypothesis's own format (if a user has both around) or, worse, *appearing* to interop while corrupting state.

**Detection:**
- Look for any code reading/writing file paths under `.hegel/`, `.hypothesis/`, or `db_root`.
- Check the hash / key derivation. PR #262 added a `native:` prefix to all database keys before hashing precisely so the native on-disk layout couldn't silently collide with a Hypothesis store sharing the same `db_root`.
- Check `Vec::with_capacity(n)` where `n` comes from deserialized input. PR #262 caught a corrupted-DB-OOM hazard from `Vec::with_capacity(count)` over an untrusted `count`.

**Remediation:** Prefix keys with a backend tag. Bound any `with_capacity` from external input by the input length. **Document the format incompatibility explicitly** in module docs — silent format divergence between two on-disk layouts that share a root path is a footgun.

## §14. Defaults that diverge between backends

Defaults that differ between native and server (even just the `Default::default()` of a settings struct) are a parity bug.

**Detection:**
```bash
git grep -nE 'phases|default_phases|verbosity|health_check|database' src/native/ src/server/ | grep -i default
```

**Examples from the audit:**
- Four call sites in `conjecture_runner.rs` used `unwrap_or_else(|| vec![Reuse, Generate, Shrink])` while the codebase-wide `Settings::new` default included `Target` and `Explicit` too (`A9`).
- `Database::Unset` mapping (covered in §3) — silent divergence.

**Remediation:** Extract a single `pub fn default_phases() -> Vec<Phase>` (or analogous helper) and have every fallback site call it. Add a test that pins the helper's contents.

## §15. Bookkeeping artefacts of the porting process

The agent kept its own files at the repo root and elsewhere. Some of these are useful (in their place — the agent's own working branch); none of them belong in an extraction PR.

**Detection — files to confirm are NOT in the PR diff:**

```bash
# Root-level agent bookkeeping:
git diff origin/main...HEAD --name-only | grep -E '^(INSTRUCTIONS|TODO|SKIPPED|FINALIZED|IMPLEMENTATION)\.(md|yaml|yml)$'

# Snapshot review artefacts:
git diff origin/main...HEAD --name-only | grep -E '\.pending-snap$'

# Agent harness scripts:
git diff origin/main...HEAD --name-only | grep -E 'scripts/(port-loop|hegelsmith-runner|setup-machine|generate_unicodedata)'

# Hegelsmith (agent's diff-testing toy):
git diff origin/main...HEAD --name-only | grep -E 'src/bin/hegelsmith/'

# Vendored Python reference (should be gitignored):
git diff origin/main...HEAD --name-only | grep -E '^resources/(pbtkit|hypothesis)/'
```

**Remediation:** Delete from the PR branch. Don't worry about losing them — they're available in `origin/DRMacIver/native` as long as you need them.

**Within the code itself, also watch for back-pointers:**
```bash
git grep -nE 'INSTRUCTIONS\.md|TODO\.yaml|SKIPPED\.md|item N[0-9]+|Refs: .* item|§[0-9]+\.[0-9]+|Tier [0SABCDE]' src/ tests/
```

Doc comments and commit messages citing audit IDs (`Refs: INSTRUCTIONS.md item N16`, `Tier-D test`, `§6.4`) make the PR read as a fragment of a long debugging exercise. Strip them — say what the code does and why, not where in the audit it was found.

## §16. Coverage hidden behind feature blackouts

The agent used `cargo-llvm-cov` runs that enabled a specific feature set, and lines exercised only under *other* feature sets appeared uncovered to the harness even when they had real tests. The agent then added `// nocov` to "fix" the coverage of code that was actually tested under a different config.

**Detection:** Compare the project's coverage script (`scripts/check-coverage.py`) against what's on this branch. PR #262 deliberately added a *dual-pass* coverage check (`3cf76c96 fix(ci): dual-pass coverage; rustfmt; drop dead From impls`) precisely to surface code that's only tested under one feature set. If the chunk you're landing reintroduces single-pass coverage, that's a regression.

```bash
git grep -nE 'cargo llvm-cov|llvm-cov' scripts/ .github/
```

**Remediation:** Don't remove the dual-pass logic. If the new code is only testable under one feature set, leave the other pass's coverage report alone — the harness aggregates. If you find old `// nocov` annotations that the dual-pass run reveals as actually-covered, delete the annotations.

---

## §17. Final sanity checks

Once you've worked through §1–§16:

1. **Read the diff again, top to bottom.** Anything you'd flag if a colleague had written it?
2. **Imagine the PR title and one-paragraph summary.** Can you describe what the PR does without referring to the porting process? If not, the PR has not yet shed its origin.
3. **Run `just check`** (formatting, lint, tests, docs) and **`just check-coverage`** (the gate). Both must pass without `// nocov` or ratchet bumps.
4. **Look at every commit message** on the PR branch. Audit-ID references, `Refs: INSTRUCTIONS.md`, `Tier-N` framing — all out. Commit messages should describe the change, not its provenance.

When all of that is clean, the chunk is ready for `landing-native-chunk` step 5 (drafting the PR).

## How to think about edge cases

The user's standing rule: **"Every time you make a decision to do something other than what the user told you to do because it is more 'pragmatic' or 'simpler', you are likely to be doing something extremely wrong."** That applies hard here. If you're tempted to:

- "Keep the `// nocov` just this once, I can't see how to test it" → restructure for testability or ask.
- "Leave the `#[ignore]`, the failure is unrelated to this chunk" → fix the failure or move it out of the chunk.
- "Bump the ratchet by 3 lines, the new code is mostly covered" → don't. Find the missing line.
- "The variable is genuinely unused but I'll `_`-prefix it just in case" → delete it.
- "This dead trait method might be useful later" → it won't be. Delete it.

Each of those moves is what the original porting agent did. The pattern of those moves is what produced the audit. Don't reproduce it.
