---
name: landing-native-chunk
description: "Extract a small, human-reviewable pull request from the DRMacIver/native branch into main. Use whenever the user asks to 'land another piece of native', 'port the next chunk', 'open a PR for <native subsystem>', or any variant on incrementally merging native-backend work. Treats DRMacIver/native as an artefact of study, not as merge material."
---

# Landing a chunk of DRMacIver/native

The `DRMacIver/native` branch implements a native (in-process) Rust backend for Hegel. It is **not** something to be merged as-is. It is an artefact: agent-written, lots of subtle wrongness, but the rough shape of where we need to land. The job is to extract one small, well-isolated PR at a time, each of which would plausibly have been written by hand.

PR [#262](https://github.com/hegeldev/hegel-rust/pull/262) (the first such chunk — integer + boolean + collections) is the model. Read its commit history before your first attempt: each review-driven cleanup commit (`review: drop det_tree…`, `fix(native): purge DataSource bypasses…`, `test(native): cover the gaps surfaced by the dual-pass coverage run`) is a worked example of the kind of bullshit you will need to find and fix in the chunk you are landing.

## The shape of one PR

A good chunk is:

- **Self-contained** — the new module(s) have few enough cross-cutting dependencies that the diff stands alone. Leaf modules first; modules that everything else depends on later.
- **Small enough that a human can review** the *whole* PR. PR #262 was ~7800 lines added, ~580 deleted, 84 files. That is at the upper end of acceptable; smaller is better. If you can split further (e.g. land the data structure separately from the consumer that needs it), do.
- **Targeted at a concrete piece of behaviour** that can be tested in this PR. Don't land machinery that will only become reachable in a future PR — it has no live caller, so its bugs are invisible.
- **Backward-compatible**: the default-features build (server backend) must keep passing every test. Anything that doesn't compile under the new feature gate gets `#[cfg(not(feature = "native"))]` or its native counterpart is added in the same PR.

## The 6-step process

### 1. Identify the chunk

Pick the smallest unit of useful work that is not yet on main. Use these signals, roughly in order:

1. **What is already cfg'd out on `native` that you can now turn on?** `git grep -n 'cfg(not(feature = "native"))' tests/ src/` on main. Each of those gates is a hook for a future PR — the test exists, it just needs the underlying engine support. If a leaf gate's prerequisites are landable in a small PR, that's your chunk.
2. **What does main `todo!()` on?** `git grep -n 'todo!' src/native/` on main. Each `todo!("schema {:?} not yet supported in native mode")` is a placeholder; filling one in is a PR-sized unit.
3. **Dependency order in `src/native/`** on `origin/DRMacIver/native`:
   - Leaf utilities (`bignum.rs`, `intervalsets.rs`, `featureflags.rs`, `dynamic_variable.rs`, `floats.rs`, `unicodedata/`) — usually portable in isolation.
   - Choice kinds (`core/choices.rs` Float / Bytes / String) — each unlocks a schema kind and a shrinker pass.
   - Schemas (`schema/{float,text,regex,special}.rs`) — sit on choice kinds.
   - Shrinker passes (`shrinker/{floats,bytes,strings,value_shrinkers}.rs`) — sit on choice kinds, exercised through the live shrinker.
   - The cache (`cache.rs`) and choicetree (`choicetree.rs`) — feed the runner.
   - Optimiser / targeting / pareto — sit on top of everything.
   - `conjecture_runner.rs` is the largest single file (~4000 lines) and the riskiest port. Leave for last; consider splitting across multiple PRs.

To look at what's still un-landed:

```bash
git diff --stat origin/main..origin/DRMacIver/native | less
git log --oneline origin/main..origin/DRMacIver/native -- <path>
```

The DRMacIver/native commit history is heavily tagged (`fix: N16 — …`, `fix: A9 — …`, etc.). Those tags are remediation items from a final-pass audit (`INSTRUCTIONS.md` in that branch); they are useful for *what bugs were found late* but **not** for picking PR units. The audit items are scattered through the whole codebase.

### 2. Worktree from `origin/main`, copy by hand

**Do not** use git operations to bring the code over. `git cherry-pick`, `git merge`, `git rebase --onto`: all unlikely to work because the history has diverged significantly from when DRMacIver/native was last rebased onto main. PR #262 itself rebased the native-minimal subset; the rest of native is from an earlier base.

Use a fresh worktree off `origin/main` so the orchestration skills here remain accessible from the original checkout (the worktree won't have them until they're merged into main):

```bash
git fetch origin
git worktree add ../hegel-rust-native-<chunk> -b DRMacIver/landing-<chunk> origin/main
cd ../hegel-rust-native-<chunk>
```

Then copy individual files from `origin/DRMacIver/native`:

```bash
# Single file:
git show origin/DRMacIver/native:src/native/<path>.rs > src/native/<path>.rs

# Whole directory tree (rare — usually you want to be selective):
git archive origin/DRMacIver/native src/native/<dir> | tar -xv -C .
```

Take **the smallest necessary subset**. Resist the urge to bring over "while I'm at it" supporting files. Each extra file is more bullshit to review.

Files that come over are *starting points*, not finished work. Expect significant rewriting.

### 3. Get something working

Compile, then test, in both feature configurations:

```bash
cargo build                                # default features (server)
cargo build --features native              # native backend
just check                                 # full gate: format + clippy + test + docs
```

You will hit:

- **Missing imports / private item references**: the agent's branch had a different module shape from main. Adjust.
- **`todo!()` for things the chunk doesn't include**: panic at `todo!("schema {:?} not yet supported in native mode")` for kinds outside the chunk's scope is fine, that's PR #262's pattern. Anything else inside scope is not a `todo!()` — it's incomplete work.
- **Tests that worked on the agent's branch because of leftover state**: e.g. tests that depend on a sibling module the agent ported but you didn't. Either bring the sibling in or rewrite the test against what's now available.

Things to **not** bring over (these are agent artefacts, not project code):

- `INSTRUCTIONS.md`, `TODO.yaml`, `SKIPPED.md`, `FINALIZED.md`, `IMPLEMENTATION.md` at the repo root — the agent's own bookkeeping for the porting process.
- `scripts/port-loop.py`, `scripts/hegelsmith-runner*.sh`, `scripts/setup-machine.sh`, `scripts/generate_unicodedata_*` — the agent's own harness.
- `src/bin/hegelsmith/` — the agent's diff-testing toy. Do **not** land this.
- `.pending-snap` files (e.g. `tests/.test_loop.rs.pending-snap`) — `insta` snapshot review artefacts that were committed by mistake. Add `*.pending-snap` to `.gitignore` if it isn't already; never commit one.
- `.claude/skills/implementing-native/`, `.claude/skills/porting-tests/`, `.claude/skills/porting-stateful/`, `.claude/skills/native-review/` — the agent's own skill files. Some of these are useful and may be worth a separate, small skills-only PR (read them critically first — they have their own bullshit). Do not bundle them with engine code.
- `resources/pbtkit/`, `resources/hypothesis/` — vendored Python reference implementations. Gitignored on main; do not commit.

### 3.5. Ungate tests that the chunk should now cover

PR #262 left a substantial pile of `#[cfg(not(feature = "native"))]` gates on tests and test files. Every chunk landed after #262 should pick up some of those gates. **A new chunk that doesn't ungate any tests is suspicious** — either the chunk is purely internal plumbing (rare), or there's gated work that should be running but isn't.

**Default to ungating, aggressively.** The bias is so far toward ungating that the project ships two annotations to make "silently still gated" impossible:

- **`#[cfg(not(feature = "native"))]`** — *permanent* exclusion. Use only for tests that are genuinely incompatible with the native backend (Python-only behaviour, server-only API surface, redundant tests of things tested elsewhere under native). These don't get re-evaluated when new engine features land.
- **`#[not_supported_on_native]`** — *temporary* xfail. An attribute proc-macro from `hegeltest-macros`, re-exported through `tests/common/mod.rs` so test files write `use common::not_supported_on_native;`. Under default features the test runs normally; under `--features native` it still runs but the harness expects it to panic (the macro injects `#[cfg_attr(feature = "native", should_panic)]`). If it doesn't panic, the *test* fails loudly with `note: test did not panic as expected`. This makes "forgot to ungate after the feature landed" impossible to miss — every new chunk's CI run automatically surfaces every previously-temporary gate whose underlying gap is now closed.

**Use `#[not_supported_on_native]` for anything that *will eventually* work on native.** Reserve `#[cfg(not(feature = "native"))]` for things that *never* will.

Process for each chunk:

1. **Inventory** the gates in the territory this chunk touches:

   ```bash
   git grep -nE 'not_supported_on_native' tests/                   # temporary xfail
   git grep -nE '#\[cfg\(not\(feature = "native"\)\)\]' tests/     # permanent + legacy item-level
   git grep -nE '#!\[cfg\(not\(feature = "native"\)\)\]' tests/    # file-level (almost always wrong post-#262)
   git grep -nE '#\[ignore' tests/                                  # bare ignores
   git grep -nE 'cfg_attr\(feature = "native"' tests/               # lint-noise suppressions
   ```

   File-level `#![cfg(not(feature = "native"))]` blocks an entire file from compiling under native — that's almost never what you actually want. Break the file open: the temporary tests get `#[not_supported_on_native]`, the permanent ones get `#[cfg(not(feature = "native"))]` per-item.

2. **For every `#[not_supported_on_native]` in scope, just run the test.** Under native CI, anything that now passes will fail-as-xfail (`note: test did not panic as expected`) — that's the signal to drop the annotation. You don't have to think about it in advance.

3. **For every `#[cfg(not(feature = "native"))]` in scope, decide:**
   - If the gate is temporary (engine feature missing), migrate to `#[not_supported_on_native]`. Future chunks then get the auto-surfacing.
   - If genuinely permanent, leave the `cfg` but add a one-line comment naming the *unrepresentable concept* (e.g. `// Native: relies on Python-side repr() formatting`, not `// TODO: native`).

4. **For tests that need engine work this chunk can do**, fix them and let them run under both backends.

5. **For tests that need engine work *this chunk* doesn't cover**, leave them as `#[not_supported_on_native]` with a one-line `// needs <specific missing piece>` comment.

6. **`#![cfg_attr(feature = "native", allow(unused_imports, dead_code))]`** at the top of integration-test files exists only because file-level `#[cfg(...)]` gates leave imports dangling. When a chunk migrates a file's gates to per-test `#[not_supported_on_native]`, the helpers actually get used under native, so the lint suppression stops doing anything — delete it.

7. **`tests/test_validation.rs`** is the home for cross-cutting "this combinator panics on bad input" tests; PR #262 gated several. If your chunk adds a new schema or panic site, check whether `test_validation.rs` has a gated entry waiting for it.

The shape of a healthy chunk: at the end of step 3.5, the diff has *more* removals of `#[not_supported_on_native]` than additions, *more* migrations of `#[cfg(not(feature = "native"))]` to the xfail annotation than the other way around, and the dangling-imports `cfg_attr` blocks shrink (or vanish) along with the gates they were compensating for.

### 4. Heavily review for bullshit

This is the most important step. Invoke the `detecting-port-bullshit` skill on the entire diff. Do not skip patterns — the agent's failure modes are systematic, and any one of them being missed will surface in human review.

Iterate: every change to fix bullshit can introduce new bullshit. Re-review what you changed.

The goal: the PR diff should read **as if a competent human wrote it from scratch**, not as if it's an extraction from a larger artefact. Specific tells of "this is an extraction" to scrub:

- Doc comments that cite the audit by ID (`Refs: INSTRUCTIONS.md item N16`, `// audit §6.4`, `// per N5`).
- pbtkit / Hypothesis pointers attached to **internal** Rust functions that have no obvious upstream — these accumulated when the agent transliterated rather than designed.
- Tests named `tier_d_<something>` or `nocover_<something>` or otherwise referencing the agent's audit taxonomy.
- Module-doc headers naming the porting process (`"Port of Hypothesis's …"` is fine when it's pointing at a specific upstream module; `"Ported by DRMacIver/native"` or `"Audited at iteration 47"` is not).

Run `just check` until it's fully clean. Run `just check-coverage`. Coverage must be 100% on new code — every uncovered line is either dead code to delete, or a test to write. **Do not add `// nocov`** without explicit human approval; "the line is hard to reach" is the prompt to restructure for testability.

### 5. Open a draft PR

**This skill overrides the standing "human writes the title and description" rule.** For native-chunk PRs the user has explicitly delegated naming to this skill — putting up a PR titled "TITLE PLACEHOLDER" with a body the user then has to rewrite is worse than putting up a PR with a plain, factual title and one-sentence description of the feature being added. Write a real title and a real description. The user will edit them if they disagree, but the default should be a usable PR, not a stub.

- **Draft, not ready**: `gh pr create --draft …`
- **Title**: short and factual — name the feature, not the marketing. "Add native float schema and shrinker", not "feat: comprehensive native float backend implementation with shrinking, validation, and tests". Match the cadence of recent merged commit subjects on `main`.
- **Body structure**:
  1. **One-paragraph human-readable summary** describing what the PR adds. Plain English, present-tense, oriented around the feature the user asked for ("Adds native byte-string support: …"). This goes *above* any `<details>` block.
  2. A clear note that the PR is generated by an LLM extracting from `DRMacIver/native`. PR #262's wording is fine: "Extracted from DRMacIver/native by Claude as part of the incremental landing of the native backend."
  3. `<details>` block with the rest: bullet list of what was brought over, what was dropped, what's stubbed with `todo!()`, what tests were added, anything surprising the human reviewer would want to know.

Example:

```bash
gh pr create --draft \
  --title "Add native floats: choice kind, schema, shrinker passes" \
  --body "$(cat <<'EOF'
Adds float support to the native backend: the `FloatChoice` choice kind in `src/native/core/choices.rs`, the `interpret_float` schema dispatch, and the `shrinker/floats.rs` pass. Default-features (server) tests are unaffected.

> Extracted from `DRMacIver/native` by Claude as part of the incremental landing of the native backend. The branch is treated as an artefact of study — this PR is a hand-reviewed subset, not a direct cherry-pick.

<details>
<summary>Extraction notes</summary>

- Brought over: <files>
- Dropped from the source branch: <agent-bookkeeping, dead-code, etc.>
- Still `todo!()` (out of scope for this PR): <list>
- New tests: <summary>
- Reviewed against the `detecting-port-bullshit` skill catalogue.

🤖 Generated with [Claude Code](https://claude.com/claude-code)
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
</details>
EOF
)"
```

Push the branch (`git push -u origin DRMacIver/landing-<chunk>`) before running `gh pr create`. Make sure the branch is based on **current** `origin/main` — rebase if main moved.

**RELEASE.md** should be a single sentence describing what the user-visible feature is, with the appropriate `RELEASE_TYPE:` header. No bullet lists, no test-plan summaries, no module-by-module breakdown — that detail belongs in the PR body's `<details>` block, not in the changelog. PR #266's first line ("The native backend (`--features native`) now supports floating-point generators.") is the model.

### 6. Watch CI; fix problems

Once the PR is open:

```bash
gh pr checks <pr-number> --watch
```

Common failures from extraction PRs (which `detecting-port-bullshit` should have caught locally, but watch for in case they didn't):

- **MSRV (1.86) breaks**: the agent's branch may have drifted to a newer Rust. Check Cargo.toml `rust-version` and any 1.87+ stdlib calls (e.g. `next_up`/`next_down` on stable, certain `Result` methods).
- **Coverage CI fails**: see the `coverage` skill. Usually means a line is unreachable from default-features tests because the new code is feature-gated; restructure so it's tested under both.
- **Windows test failures**: `cfg(unix)` gates around `UnixStream` etc. (PR #262's `fix(native): gate feature-binding tests + cfg(unix) embedded tests` is the template).
- **`uv` not on PATH on Windows runners**: `TempRustProject`-based tests spawn a default-features subprocess that needs the server backend, which needs uv. The `install-tools` action should provide it; if it doesn't on the new job, add `just uv` to the new matrix entry.
- **`test-all-features`** runs both `native` and other backend-incompatible features; gate appropriately.
- **`check-test-modules.py`** orphan-detector trips on a new `tests/<dir>/` file not declared in `main.rs`. Add the `mod` declaration.
- **Coverage ratchet**: if your PR's coverage is slightly worse than the ratchet, **do not bump the number** without asking. Find the missing test.

When CI is green, the PR is ready for human review — the user will mark it ready-for-review (or ask for changes).

## Relationship to other skills

- **`detecting-port-bullshit`** — the catalogue of agent failure modes to scrub for. Invoked from Step 4.
- **`self-review`** — generic pre-PR checks. Run after `detecting-port-bullshit`, before opening the PR.
- **`coverage`** — coverage rules and ratchet philosophy. Most extraction PRs will brush up against the coverage gate at least once.
- **`changelog`** — `RELEASE.md` style guide. Add a line for the new feature.
- **`new-generator` / `new-default-generator` / `add-library-support`** — irrelevant to native-engine ports; they cover third-party-crate integrations.

## What not to do

- **Do not** treat the DRMacIver/native commit log as a recipe. The audit-tag commits (`fix: N16`, `fix: A12`) are remediation patches in the middle of a long debug, not landable units.
- **Do not** `git cherry-pick` from DRMacIver/native. The base has diverged; you'll get either a conflict mess or a silently-broken merge.
- **Do not** land speculative machinery. Code with no live caller in *this PR* gets bugs that *this PR's review* can't catch. Land the consumer in the same PR, or wait.
- **Do not** ship "this is mostly the agent's work" tells (audit-ID references, INSTRUCTIONS.md back-pointers, `// per audit §6.4` comments). Reviewers should not have to know the porting process existed.
- **Do not** mark the PR ready-for-review. It's the user's job to flip from draft.
