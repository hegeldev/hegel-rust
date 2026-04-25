#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = ["pyyaml>=6"]
# ///
"""Self-driving loop that runs gates, clears TODOs, then ports upstream tests.

Outer loop, each iteration:
  1. `cargo clean` so the build starts from cold.
  2. repair(): `just format` + `cargo clippy --fix` (auto-committed if the
     tree was clean before), then `just lint`, `cargo test`,
     `HEGEL_SERVER_COMMAND=/bin/false cargo test --features native`, clean
     tree — each as a gate that dispatches claude on failure.
  3. Sync with origin: `git fetch`, rebase the current branch onto
     `origin/main`, and push (auto-pushes any local commits). Rebase
     conflicts or push failures dispatch a claude agent.
  4. Gate on the upstream PR (hegeldev/hegel-rust#188)'s CI. Block on
     `gh pr checks --watch` while checks are pending. If the cycle
     completes red, dispatch a short TRIAGE agent that reads an
     already-extracted failing-log summary and prepends one `[CI]`-
     prefixed TODO entry per independent failure to TODO.yaml (it does
     not fix anything). The entries are then cleared one-per-dispatch
     by step 5. If TODO.yaml already has `[CI]` entries when CI goes
     red, skip re-triage and let them drain first.
  5. If `TODO.yaml` has any entries, pop the first one and dispatch claude
     to clear it, then continue the outer loop (repair runs again before
     the next action). CI failures flow through this path via step 4's
     triage.
  6. If no TODOs, pick a random unported upstream file and enter the port
     sub-loop.

Port sub-loop (one pick; the outer gates are skipped while this runs):
  a. upstream file in SKIPPED.md → sub-loop done
  b. destination file exists with at least one `#[test]`
  c. `cargo test --test {kind} {module}` passes
  d. `HEGEL_SERVER_COMMAND=/bin/false cargo test --features native
       --test {kind} {module}` passes
  e. working tree clean
  f. once a-e all pass in a single pass and commits have been made during
     this sub-loop, dispatch claude to review the port; if the reviewer
     makes any new commits, the sub-loop restarts to re-verify. If the
     reviewer makes no changes, sub-loop done.

On the first failing check (outer or inner), claude is invoked with a focused
prompt and the same loop restarts at the top. When every upstream file is
accounted for (ported or in SKIPPED.md), TODO.yaml is empty, and the outer
gates all pass, the loop exits 0.

Run via `uv run scripts/port-loop.py` (uv reads the PEP 723 header above and
provisions PyYAML automatically).
"""

from __future__ import annotations

import argparse
import difflib
import hashlib
import json
import os
import random
import re
import shutil
import signal
import subprocess
import sys
import threading
import time
from pathlib import Path

import yaml


# ---- prompts (tune freely) ---------------------------------------------------

COMMON_SYSTEM_PROMPT = """\
You are driven by scripts/port-loop.py — a non-interactive loop that
invokes you with one focused task per call. Do the task, commit, exit.
The loop re-runs its gates after you return; a partial fix is fine, the
next invocation picks up from the next failing gate.

Ground rules:
- TDD when fixing bugs: regression test first.
- Commit each focused change. Never --amend, never --no-verify.
- Do NOT run `git push`. The port-loop script manages pushing; your
  commits will be picked up and pushed (or cherry-picked) by the
  supervisor. Pushing from here can land half-done work on the remote.
- Before porting or reviewing a port, read
  .claude/skills/porting-tests/SKILL.md.
- Before touching src/native/ (including filling a todo!() stub or
  native-gating a test that needs new engine support), read
  .claude/skills/implementing-native/SKILL.md. The native engine is a
  port of Hypothesis's semantics. Hypothesis
  (resources/hypothesis/hypothesis-python/src/hypothesis/internal/) is
  the behavioural ground truth; pbtkit
  (resources/pbtkit/src/pbtkit/) is a cleaner reference implementation
  of the same core ideas and is usually the easier read. When they
  conflict, match Hypothesis.
"""

# Appended to port-related prompts only (not to the common system prompt)
# so non-port dispatches don't pay to cache-read it every turn.
SKIP_POLICY = """\

Skip vs. port policy:

- Add a file to SKIPPED.md ONLY when its tests rely on *public API* with
  no hegel-rust counterpart: Python-specific facilities (pickle, __repr__,
  sys.modules, Python syntax, dunder access) or integrations with other
  Python libraries (numpy, pandas, django, attrs, redis).

- "Has no Rust counterpart" is NOT on its own a valid reason to skip. It
  is the reason to PORT. Tests on internal APIs (pbtkit/Hypothesis engine
  internals) exist to pin down behaviour that src/native/ must match. If
  the native feature doesn't exist yet: native-gate the test with
  `#[cfg(feature = "native")]` and add the feature under src/native/
  (stubbed with `todo!()` if too large for one commit). The test must
  compile in both modes; `todo!()` goes in the source, never the test.

- Do NOT skip tests on the grounds that hegel-rust has "an equivalent
  elsewhere" or "this is redundant". Redundancy is fine. A later
  rationalisation pass deduplicates; don't pre-empt it.
"""

LINT_FIX_PROMPT = (
    "`just lint` is failing. The full output is included below — work from "
    "it instead of rerunning the command. Fix the lints and commit."
)

SERVER_TEST_FIX_PROMPT = (
    "`cargo test` is failing. The full output is included below — work from "
    "it instead of rerunning the command. Fix the first failing test and "
    "commit. Don't bundle other fixes in the same commit."
)

NATIVE_TEST_FIX_PROMPT = (
    "`HEGEL_SERVER_COMMAND=/bin/false cargo test --features native` is "
    "failing. The full output is included below — work from it instead of "
    "rerunning the command. Fix the first failing test and commit."
)

TEST_PERF_FIX_PROMPT = """\
A `cargo test` run timed out and was killed by the port-loop harness.
The partial output below ends with a `*** port-loop: ... timed out
after Ns and was killed ***` banner that names the exact command.

This is a performance problem, not a correctness one. The root cause
may be in the tests themselves (doing too much generation, recompiling
a fresh cargo project per test, etc.) or in the library code the tests
exercise. Identify the slow test(s) — the `--nocapture --test-threads=1`
output or a quick `cargo test -- -Z unstable-options --report-time`
will surface them — understand WHY they're slow, and fix the root
cause.

Performance target:
- Every `#[test]` should ideally run in < 1 second and definitely
  < 5 seconds. Anything above that is a bug to be fixed.
- `TempRustProject` compiles a fresh cargo project per call and is the
  usual suspect when a batch of tests blows the budget; look at sharing
  compile artefacts or moving compile-only assertions to `trybuild`.

Ground rules:
- Do NOT "fix" this by marking tests `#[ignore]`, deleting them, or
  adding them to SKIPPED.md.
- Do NOT raise the port-loop timeout.
- Commit the fix. The next iteration re-runs the same gate with the
  same timeout and will either green-light it or re-dispatch you with
  fresh output.
"""

TEST_SLOW_FIX_PROMPT = """\
A `cargo test` run completed (all tests passed overall), but libtest
emitted one or more `test <name> has been running for over N seconds`
warnings. Those lines are preserved in the output below — search for
`has been running for over` to find them.

This is a performance problem even though the tests passed. A test that
takes over 60 seconds is a bug regardless of the final result; it slows
the feedback loop, stresses CI, and usually indicates the test is doing
far more work than it should.

Performance target:
- Every `#[test]` should ideally run in < 1 second and definitely
  < 5 seconds. Anything above 60 seconds is always a bug to be fixed.
- `TempRustProject` compiles a fresh cargo project per call and is the
  usual suspect when a batch of tests blows the budget; look at sharing
  compile artefacts or moving compile-only assertions to `trybuild`.

For each flagged test: identify it, understand WHY it's slow (profile
it if needed), and fix the root cause — in the test itself or in the
library code it exercises.

Ground rules:
- Do NOT "fix" this by marking tests `#[ignore]`, deleting them, or
  adding them to SKIPPED.md.
- Do NOT suppress libtest's "has been running for over" warning.
- Commit the fix. The next iteration re-runs the same gate and will
  re-dispatch you if any test still trips the warning.
"""

CI_TRIAGE_PROMPT = """\
CI on https://github.com/{repo}/pull/{pr} is red. Your ONLY job in this
dispatch is triage: break the failing-job logs below into independent
TODO entries and prepend them to TODO.yaml. Do NOT try to fix anything
in this invocation — a later dispatch will pick each TODO off the top
and fix it.

The log extract below is already filtered (panics, assertion failures,
test-result lines, compile errors, coverage-ratchet failures). Read it,
identify the distinct root causes, and write ONE TODO entry per
independent root cause. Guidelines:

- Title: start with `[CI] ` and name the failing test or check.
  Example: `[CI] tests/test_native.rs::native_regex_unicode_...`.
- If two failures share a root cause (same test, same panic),
  merge into one entry — don't split cosmetically.
- `details`: include the relevant extracted log lines verbatim so the
  fix agent doesn't need `gh run view`. Also note the failing command
  or workflow job if clear from the log.
- Prepend the new entries to the top of TODO.yaml (above existing
  entries). Keep existing entries and their order otherwise untouched.
- Do NOT modify any other files. Do NOT fix the failures. Do NOT run
  tests. Just edit TODO.yaml.

One focused commit: "Triage CI failures on PR {pr}" or similar.
"""

SYNC_FIX_PROMPT = """\
The port loop was unable to rebase the current branch onto origin/main
and push it (the previous step in the iteration). Output of the failed
command(s) is included below.

Common causes:
- Rebase conflict: resolve the conflict, `git rebase --continue`, then
  push with `--force-with-lease`. If the conflict is more involved than
  a quick resolution, investigate — it may indicate the local work has
  drifted from upstream in a way that needs a human call.
- Push rejected because origin moved: fetch again (`git fetch origin`)
  and re-inspect; someone else may have pushed to the same branch. If
  the remote has commits you don't: pull/rebase them in. If it's a
  `--force-with-lease` mismatch that looks safe, proceed with
  `git push --force-with-lease` once the lease matches.
- Missing upstream ref: run `git push --set-upstream origin HEAD` if
  this branch has never been pushed.

Do NOT `git push --force` blindly — prefer `--force-with-lease`. Do
NOT rebase or force-push `main`/`master` under any circumstances.

After resolving, the next port-loop iteration will re-run the sync
gate; a clean run there moves the loop on to the PR CI check.
"""

COMMIT_PROMPT = (
    "All gates pass but the working tree is dirty. `git status --porcelain` "
    "output is included below. Make a focused commit describing the change, "
    "or stash/revert if the diff was accidental."
)

PORT_PROMPT = """\
Port the upstream test file {path} to {destination} per the porting-tests
skill. You will be invoked repeatedly on this file until it's green in
both server and native mode with a clean tree, or (per the skip policy
below) `{name}` is in SKIPPED.md. Make one focused commit toward that
goal.
""" + SKIP_POLICY

PORT_TEST_FIX_SERVER_PROMPT = """\
Continuing port {path} → {destination}. The module's server-mode tests
are failing: `cargo test --test {kind} {module}`. Full output below —
work from it rather than rerunning the command. Fix the failing tests
(or the ported module) and commit.
"""

PORT_TEST_FIX_NATIVE_PROMPT = """\
Continuing port {path} → {destination}. The module's native-mode tests
are failing: `HEGEL_SERVER_COMMAND=/bin/false cargo test --features
native --test {kind} {module}`. Full output below — work from it
rather than rerunning the command. Fix the failing tests and commit.

Missing native-mode engine features are NOT a reason to add this file to
SKIPPED.md (see skip policy below). Instead: native-gate the affected
test(s) with `#[cfg(feature = "native")]` and add the missing feature
under `src/native/` — stubbed with `todo!()` if it's too large for one
focused commit. When implementing or stubbing: read
.claude/skills/implementing-native/SKILL.md first. Hypothesis
(`resources/hypothesis/hypothesis-python/src/hypothesis/internal/`) is
the behavioural ground truth; pbtkit (`resources/pbtkit/src/pbtkit/`)
is a cleaner reference implementation of the same core ideas. When
they conflict, match Hypothesis.
""" + SKIP_POLICY

PORT_COMMIT_PROMPT = """\
Continuing port {path} → {destination}. Filtered tests pass in both
server and native mode, but the working tree is dirty. `git status
--porcelain` output below. Make a focused commit.
"""

PORT_MISSING_TESTS_PROMPT = """\
Continuing port {path} → {destination}. The destination file exists
but contains no `#[test]` attribute — either the port is incomplete
or stubbed out. Add the ported tests and commit. Review the skip
policy below before routing this file to SKIPPED.md; it is strict.
""" + SKIP_POLICY

PORT_SKILL_UPDATE_PROMPT = """\
Reflection pass for the port of {path} → {destination}. The gates are
green and commits have been made ({start_sha}..HEAD below). Before the
reviewer looks at this, update the skills so future porting agents
benefit from anything non-obvious this port surfaced.

Existing skills live under `.claude/skills/`:
  - changelog/             — release-note conventions
  - coverage/              — coverage philosophy + ratchet
  - implementing-native/   — adding/extending src/native/ features
  - native-review/         — auditing native code after it lands
  - porting-tests/         — porting Python PBT tests to Rust
  - self-review/           — pre-commit self-audit

Read the relevant skill file(s) and the diff ({start_sha}..HEAD). Then
decide — honestly — whether any of the following apply:

- A Python→Rust translation pattern that was non-obvious and isn't
  already documented (goes in
  `.claude/skills/porting-tests/references/api-mapping.md`).
- A pbtkit or Hypothesis convention that tripped you up (fixture
  style, hook ordering, conftest.py setup, parametrize shapes,
  unusual imports) — add to the relevant reference under
  `.claude/skills/porting-tests/references/`.
- A native-implementation insight (how to structure a new
  `src/native/` module, where pbtkit and Hypothesis diverge on a
  feature, a subtle invariant you had to preserve) — belongs in
  `.claude/skills/implementing-native/SKILL.md`.
- Something large enough, distinct enough, or recurring enough that
  it deserves its own skill. If so, create
  `.claude/skills/<new-skill>/SKILL.md` with the standard YAML
  frontmatter:

      ---
      name: <new-skill>
      description: "<one-line summary of when an agent should use this>"
      ---

  and cross-link it from any existing skills that should mention it.

If you add or change skills, commit the edits as a separate focused
commit ("Update <skill> with <thing>" or similar). Don't duplicate
what's already there; amend in place. If nothing from this port is
novel enough to document, reply with one short sentence explaining
why and make no commits — the loop will continue to the review
step either way.

Commits in this sub-loop:

{log}
"""

PORT_RESCUE_PROMPT = """\
Rescue task: the parallel port-loop worker spent its full per-file
dispatch budget ({dispatches} attempts) trying to port {path} →
{destination} and could not get all gates green. Instead of continuing
to bang on the same port, abandon it cleanly:

1. Read the tail of the worker's session log (below) and identify what
   actually failed — the concrete gate(s), the error messages, the
   nature of the obstacle.

2. Decide between SKIPPED.md and TODO.yaml:
   - If the blocker is genuinely a skip-worthy public-API gap (Python
     specific facilities, external-library integrations — see the skip
     policy below), add `{name}` to the appropriate section of
     SKIPPED.md with a one-line rationale citing the actual failure.
   - If the blocker is a missing native-mode engine feature, an unclear
     porting pattern, or anything else the loop could reasonably pick
     up later with a cleaner state: add a TODO.yaml entry describing
     what needs to happen before this file can be ported, then ALSO add
     `{name}` to SKIPPED.md with a rationale pointing at the TODO so
     the file stays out of the unported pool until a human unblocks it.

3. Revert any uncommitted changes to unrelated files so the tree is
   clean apart from your rescue commit. Do not keep half-done port
   work on disk.

4. Make one focused commit that records the abandonment ("Skip {name}:
   <reason>" or similar), then exit.

Do NOT attempt a further port. Do NOT claim the port is complete. The
budget existed precisely to catch ports that get stuck; your job is to
record the obstacle and let the loop move on.

Session log tail (most recent {log_lines} lines):

{session_tail}
""" + SKIP_POLICY

POST_REBASE_FIX_PROMPT = """\
Parallel port-loop worker: after porting {path} → {destination} and
reaching green gates, the worker rebased its branch onto
`origin/{supervisor_branch}` and the gates are now broken again —
something on the supervisor branch changed in a way that conflicts
with (or regresses) the port. Full gate output is below.

Fix the regression in one focused commit. The worker will re-run the
gates; if still broken after this attempt, the worker escalates to a
rescue agent which will abandon the port rather than loop forever.
"""

POST_REBASE_CONFLICT_PROMPT = """\
Parallel port-loop worker: a rebase of the worker branch onto
`origin/{supervisor_branch}` hit conflicts. `git status` is included
below. You are now mid-rebase.

Resolve the conflict(s) and drive the rebase to completion:

- Inspect the conflicted files and decide the right content for each
  hunk. For `SKIPPED.md`, the usual resolution is a union — keep both
  workers' entries in the same section. For source/test files, prefer
  the version that matches what the port actually needs; drop stale
  edits.
- `git add <file>` each resolved file (or `git rm` if the file is
  meant to be deleted).
- `git rebase --continue` until the rebase is done. If more conflicts
  surface, repeat.
- If a commit becomes empty after resolution, use `git rebase --skip`.
- Do NOT `git rebase --abort` unless the conflict is genuinely
  unresolvable and the whole port needs to be redone from scratch.

After this, the worker re-runs the native + server gates. If those
pass, the port is complete. If they fail, a follow-up dispatch gets
one chance to fix the regression before the port is abandoned via
SKIPPED.md.

Rebase-state output:
"""

PORT_REVIEW_PROMPT = """\
Review the port of {path} → {destination}. The gate chain (destination
exists, has `#[test]` attributes, server-mode tests pass, native-mode
tests pass, working tree clean) is currently green, and a skill-update
reflection pass has already run. Below is the list of commits made
during this sub-loop ({start_sha}..HEAD).

Read the upstream file ({path}), the ported file ({destination}), and
the commits under review. Then evaluate honestly, applying the skip
policy below:

- Was a real, faithful attempt made to port the tests, or were corners
  cut? Watch for: tests stubbed out, assertions weakened, whole test
  cases silently dropped, behavior papered over with `assume!`,
  failures hidden behind `#[ignore]`, or `todo!()` placed in the test
  body rather than in the native-mode source code it should be
  driving.
- Was anything pushed to SKIPPED.md, or silently dropped from the port
  (listed as "omitted" in the module docstring), that should actually
  have been ported? The skip policy only covers public-API
  incompatibility (Python-specific facilities, external-library
  integrations). Engine internals and missing native-mode features
  are NOT skip-worthy — they should be native-gated in the test and
  stubbed under `src/native/`. In particular, watch for these common
  mistakes:
  * Tests omitted because "hegel-rust has no counterpart for this
    internal API" — that's exactly the native-gated-plus-source-stub
    case; port them.
  * Tests omitted on the grounds that they're "already covered" by
    some other Rust test or "redundant" — redundancy is fine, mis-skips
    are not. A later rationalisation pass handles deduplication; don't
    pre-empt it. Restore any such tests.
  If a skip or drop was miscategorised, revert it and port properly.
- Are there improvements worth making to clarity, naming, idiom, or
  code quality in the ported file or anything it touched?
- Is the coverage of the upstream behavior adequate given hegel-rust's
  available API? If missing cases could be added by native-gating plus
  a source-level stub, add them.
- If the skill-update pass added or changed a skill file: is the
  change accurate, terse, and non-duplicative? Revert or tighten if
  not. If the skill-update pass made no commits but obvious
  patterns went undocumented, do it now.

If you find anything worth changing, make focused commits to fix it —
the sub-loop will re-run the gates afterward and invoke you again if
anything is broken. If the port is genuinely good as-is, reply with a
short confirmation and make no commits; the sub-loop will then move
on.

Commits under review:

{log}
""" + SKIP_POLICY

TODO_PROMPT = """\
Clear the following TODO entry from TODO.yaml (at the repo root). The port
loop dispatches TODO entries one at a time; each invocation is expected to
handle ONE entry.

Entry:

{entry}

({remaining} other entries will remain in TODO.yaml after this one.)

BEFORE doing any work, first check whether the entry's acceptance criteria
are already satisfied. Port-loop agents have touched this repo many times
and some TODO entries describe work that has since landed under a different
framing. Grep for the files/functions/tests the entry names, check recent
git log, and check SKIPPED.md. If the work is already done, delete the
entry from TODO.yaml with a one-line commit explaining what you observed.

When the work is done:
- Remove THIS entry (and only this entry) from TODO.yaml.
- Commit the code changes and the TODO.yaml edit together. Multiple focused
  commits are fine if the work naturally splits; just make sure the final
  commit removes the entry so the loop knows it's cleared.

If the work is larger than one invocation, replace this entry in TODO.yaml
with one or more narrower follow-ups that together cover the remaining
work. Don't leave the original entry in place.

If you realise the TODO is wrong, already done, or a bad idea, remove it
anyway with a commit that explains the decision.

You may also add new TODO entries if you notice things that should be done
along the way — anywhere in the list, not just at the end. Keep new entries
focused and use the same `title` / `details` schema as existing ones.
"""

TODO_RECOVERY_PROMPT = """\
Recovery task: the port loop has dispatched {attempts} previous attempts at
the following TODO entry from TODO.yaml, and the entry is still in the
queue. Something is wrong with the entry itself, or with how agents keep
approaching it.

Entry:

{entry}

Figure out what's going on and act on it. Options (pick whichever fits):

1. The entry is too broad / badly framed — rewrite it in place, or replace
   it with one or more narrower follow-ups that each fit in a single
   invocation. Commit the TODO.yaml edit with a message explaining the
   rewrite.

2. The entry is already (partly) done and prior attempts couldn't find a
   natural commit to make — remove or rewrite the entry accordingly, and
   commit.

3. The work is genuinely not worth doing (wrong, obsolete, or a bad idea) —
   remove the entry with a commit message explaining the decision.

4. The work is blocked by something not capturable as a TODO — move the
   affected piece to SKIPPED.md (under the appropriate section, with a
   one-line rationale), shrink or drop the TODO entry to match, and
   commit.

Read the recent git log (`git log -n 20 --oneline`) to see what the
previous attempts actually committed; that's often the clearest signal
for why they stalled. Whatever you choose, the TODO.yaml state MUST
change in this invocation so the loop doesn't keep hammering the same
unchanged entry.
"""

# ---- finalize-mode prompts ---------------------------------------------------

FINALIZE_PROMPT = """\
Integrate the ported test file {path} into the hegel-rust test suite.

You will be invoked repeatedly until {path} no longer exists and its stem
({stem}) appears in FINALIZED.md. Make one focused commit toward that goal.

Steps:
1. Read {path} carefully. Understand what each test covers.
2. Search tests/ (excluding tests/hypothesis/ and tests/pbtkit/) for existing
   tests that cover the same ground. Remove duplicates from {path}.
3. Move the remaining unique tests to appropriate locations in tests/:
   - If they fit an existing file logically, add them there.
   - If they warrant a new file, create one with a feature-oriented name (not
     the upstream Python source name).
   - Use `use hegel::generators as gs;` imports throughout.
4. Once all tests are relocated, delete {path} and update
   tests/{kind}/main.rs if it declares the module.
5. Add a line to FINALIZED.md:
     - {stem} — <one-line summary of where the tests ended up>
6. Commit everything in one focused commit.

The loop re-runs gates after you return; partial progress is fine.
"""

FINALIZE_FIX_PROMPT = """\
The test suite is broken after integrating {path}. Full output is below —
work from it rather than rerunning the command. Fix the failing tests and
commit.
"""

FINALIZE_RECORD_PROMPT = """\
The file {path} has been removed but {stem} is not yet recorded in
FINALIZED.md. Add the line:
  - {stem} — <one-line summary of what was done>
to FINALIZED.md and commit.
"""


# ---- gate helpers ------------------------------------------------------------


REPO_ROOT = Path(__file__).resolve().parent.parent
FINALIZED_MD = REPO_ROOT / "FINALIZED.md"
# `resources/` is gitignored, so worker worktrees don't have it. Workers
# set PORT_LOOP_RESOURCES_BASE=<supervisor-repo-root> and read upstream
# test files from there. Destinations (tests/pbtkit/*.rs) still land in
# REPO_ROOT (the worker's worktree).
_RESOURCES_BASE = Path(os.environ.get("PORT_LOOP_RESOURCES_BASE") or REPO_ROOT)
PBTKIT_DIR = _RESOURCES_BASE / "resources" / "pbtkit" / "tests"
HYPOTHESIS_DIR = (
    _RESOURCES_BASE
    / "resources"
    / "hypothesis"
    / "hypothesis-python"
    / "tests"
)
SESSIONS_DIR = REPO_ROOT / ".porting" / "sessions"

# Upstream PR whose CI the loop babysits. If CI on this PR has completed
# as failed, the loop pauses everything else and dispatches a specialised
# fix agent until the PR is green (or at least pending again).
PR_NUMBER = 188
PR_REPO = "hegeldev/hegel-rust"

# libtest emits `test <name> has been running for over N seconds` when a
# test has exceeded a 60-second soft threshold. We treat this as a
# performance failure even when the overall test exit code is clean.
SLOW_TEST_RE = re.compile(
    r"^\s*test \S+ has been running for over \d+ seconds?\s*$"
)

# Lines we strip from gate output before feeding it back to a fix agent.
# These are cargo/libtest noise that eats tokens without informing the
# fix — compile progress, per-test "ok" lines, the "running N tests"
# preamble. The human watching the terminal still sees everything live;
# only the agent prompt is stripped.
_CARGO_PROGRESS_PREFIXES = (
    "Compiling ",
    "Finished ",
    "Running ",
    "Blocking ",
    "Checking ",
    "Fresh ",
    "Updating ",
    "Downloaded ",
    "Downloading ",
    "Locking ",
    "Adding ",
    "Building ",
    "Packaging ",
    "Documenting ",
)
_ANSI_RE = re.compile(r"\x1b\[[0-9;]*[a-zA-Z]")
_PASSING_TEST_RE = re.compile(
    r"^\s*test \S+(?: - should panic)? \.\.\. ok\s*$"
)
_RUNNING_PREAMBLE_RE = re.compile(r"^\s*running \d+ tests?\s*$")
_CC_LINKER_NOTE_RE = re.compile(r'^\s*=\s+note:\s+"cc"\s+"')
_LD_LINE_RE = re.compile(r"/usr/bin/ld:")


_MAX_GLOBAL_OCCURRENCES = 3
_MAX_LD_LINES = 15


def strip_build_noise(output: str) -> str:
    """Remove cargo progress + passing-test lines from gate output.

    Keeps failing tests, panic/backtrace, slow-test warnings, compile
    errors, clippy output, test-result summaries, and everything else
    that could help an agent diagnose the failure. Also:
    - strips ANSI escape codes so regexes match colored cargo output,
    - collapses runs of consecutive identical lines (common with
      `/usr/bin/ld:` spam from linker failures) behind a counter,
    - caps each identical non-blank line at `_MAX_GLOBAL_OCCURRENCES`
      across the whole output, dropping further repeats silently —
      cargo frequently repeats the same linker/compile error for every
      crate in the graph, and the agent only needs to see it once,
    - truncates multi-kilobyte `= note: "cc" "..."` linker-command
      dumps that follow `error: linking with cc failed` — the
      invocation is never what the agent needs; the `ld:` errors
      below it are.
    - caps `/usr/bin/ld:` lines at `_MAX_LD_LINES` globally: each
      line is unique (different rlib hashes, symbol addresses), but
      cargo emits essentially the same linker failure once per crate
      in the dependency graph, so after ~15 samples further ld lines
      are dropped silently.
    - collapses runs of blank lines.
    """
    kept: list[str] = []
    prev_blank = False
    prev_line: str | None = None
    dup_count = 0
    global_count: dict[str, int] = {}
    dropped_global: dict[str, int] = {}
    ld_count = 0
    ld_dropped = 0

    def flush_dups() -> None:
        nonlocal dup_count
        if dup_count:
            kept.append(
                f"  (... previous line repeated {dup_count} more times ...)"
            )
            dup_count = 0

    for raw in output.splitlines():
        line = _ANSI_RE.sub("", raw).rstrip()
        stripped = line.lstrip()
        if any(stripped.startswith(p) for p in _CARGO_PROGRESS_PREFIXES):
            continue
        if _PASSING_TEST_RE.match(line):
            continue
        if _RUNNING_PREAMBLE_RE.match(line):
            continue
        if _CC_LINKER_NOTE_RE.match(line) and len(line) > 200:
            line = (
                line[:120].rstrip() + " ... (cc invocation truncated)"
            )
        if _LD_LINE_RE.search(line):
            if ld_count >= _MAX_LD_LINES:
                ld_dropped += 1
                continue
            ld_count += 1
        if line == prev_line:
            dup_count += 1
            continue
        flush_dups()
        prev_line = line
        # Global occurrence cap (only applies to non-blank content).
        if line.strip():
            seen = global_count.get(line, 0)
            if seen >= _MAX_GLOBAL_OCCURRENCES:
                dropped_global[line] = dropped_global.get(line, 0) + 1
                continue
            global_count[line] = seen + 1
        if not line.strip():
            if prev_blank:
                continue
            prev_blank = True
        else:
            prev_blank = False
        kept.append(line)
    flush_dups()
    if ld_dropped:
        kept.append(
            f"\n  (... {ld_dropped} further `/usr/bin/ld:` line(s) omitted "
            f"after the first {_MAX_LD_LINES}; the linker emits essentially "
            f"the same failure once per crate ...)"
        )
    if dropped_global:
        total = sum(dropped_global.values())
        uniq = len(dropped_global)
        kept.append(
            f"\n  (... {total} further repeats of {uniq} previously-shown "
            f"line(s) omitted; each is quoted above at least "
            f"{_MAX_GLOBAL_OCCURRENCES} times ...)"
        )
    return "\n".join(kept)

# Hot-reload bookkeeping: captured once at import, checked at the top of
# each main-loop iteration. If this script's mtime changes mid-run
# (e.g. during a `git pull` or while the user is iterating on the
# driver itself), re-exec into the new version with the original argv
# so the long-running loop doesn't need to be restarted by hand.
SCRIPT_PATH = Path(__file__).resolve()
SCRIPT_MTIME_AT_STARTUP = SCRIPT_PATH.stat().st_mtime
ORIGINAL_ARGV: list[str] = list(sys.argv)


class _LinePrefixStream:
    """Wrap a text stream, prefixing every output line with `prefix`.

    Used by workers in parallel mode so interleaved supervisor output
    stays attributable. Buffers partial lines across writes to avoid
    mid-line prefix insertions.
    """

    def __init__(self, inner, prefix: str) -> None:
        self._inner = inner
        self._prefix = prefix
        self._buf = ""

    def write(self, s: str) -> int:
        if not self._prefix:
            return self._inner.write(s)
        self._buf += s
        while "\n" in self._buf:
            line, self._buf = self._buf.split("\n", 1)
            self._inner.write(f"{self._prefix}{line}\n")
        return len(s)

    def flush(self) -> None:
        if self._buf:
            self._inner.write(f"{self._prefix}{self._buf}")
            self._buf = ""
        self._inner.flush()

    def isatty(self) -> bool:
        return getattr(self._inner, "isatty", lambda: False)()

    def fileno(self) -> int:
        return self._inner.fileno()

    def __getattr__(self, name):
        return getattr(self._inner, name)


def _install_log_prefix(prefix: str) -> None:
    """Wrap `sys.stdout`/`sys.stderr` so every line gets a `[prefix] ` tag.

    No-op when `prefix` is empty. Called once at worker startup.
    """
    if not prefix:
        return
    tag = f"[{prefix}] "
    sys.stdout = _LinePrefixStream(sys.stdout, tag)
    sys.stderr = _LinePrefixStream(sys.stderr, tag)


def maybe_hot_reload() -> None:
    """If port-loop.py has been modified since startup, re-exec into it.

    Called at the top of each iteration of the long-running outer loops
    (default and `--todo-only` modes), before any expensive work.
    Per-process state (iteration count, TODO attempts, last session id)
    is discarded on re-exec — that's intentional: a new version of the
    script starts clean.

    Only reloads when the on-disk script matches HEAD — i.e. the change
    has been committed. Uncommitted edits (work-in-progress by a human
    or by the port-loop's own agents) are ignored so we don't re-exec
    into a half-saved version.

    Uses the shebang's `uv run --script` line via `os.execv` on the
    script path, so PEP 723 deps get re-resolved if they changed.
    """
    try:
        current = SCRIPT_PATH.stat().st_mtime
    except OSError:
        return
    if current == SCRIPT_MTIME_AT_STARTUP:
        return
    status = subprocess.run(
        ["git", "status", "--porcelain", "--", str(SCRIPT_PATH)],
        cwd=REPO_ROOT,
        capture_output=True,
        text=True,
    )
    if status.returncode != 0 or status.stdout.strip():
        # Uncommitted changes (or we couldn't ask git): don't reload yet.
        return
    print(
        f"\n[port-loop] {SCRIPT_PATH.name} changed on disk "
        f"(mtime {SCRIPT_MTIME_AT_STARTUP:.0f} → {current:.0f}) "
        f"and matches HEAD; re-exec'ing with argv {ORIGINAL_ARGV!r}.",
        flush=True,
    )
    os.execv(str(SCRIPT_PATH), ORIGINAL_ARGV)


def run_gate(
    cmd: list[str],
    *,
    env: dict[str, str] | None = None,
    timeout: float | None = None,
) -> tuple[int, str, str | None]:
    """Run a gate command, stream output live, return (exit_code, output, perf).

    `perf` is `"timeout"` if the command was killed by the port-loop timer,
    otherwise `None`. Test-specific gates post-process the captured output
    to additionally detect libtest slow-test warnings (→ `perf = "slow"`).
    """
    print(f"\n$ {' '.join(cmd)}")
    if timeout is not None:
        print(f"[port-loop] timeout: {timeout:.0f}s", flush=True)
    proc = subprocess.Popen(
        cmd,
        cwd=REPO_ROOT,
        env=env,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
        bufsize=1,
    )
    captured: list[str] = []
    assert proc.stdout is not None

    def _reader() -> None:
        assert proc.stdout is not None
        for line in proc.stdout:
            sys.stdout.write(line)
            sys.stdout.flush()
            captured.append(line)

    t = threading.Thread(target=_reader, daemon=True)
    t.start()
    timed_out = False
    try:
        proc.wait(timeout=timeout)
    except subprocess.TimeoutExpired:
        timed_out = True
        proc.kill()
        proc.wait()
    t.join()
    output = "".join(captured)
    if timed_out:
        banner = (
            f"\n\n*** port-loop: `{' '.join(cmd)}` timed out after "
            f"{timeout:.0f}s and was killed. Output above is only what was "
            f"captured before the timeout. ***\n"
        )
        print(banner, flush=True)
        return 124, output + banner, "timeout"
    return proc.returncode, output, None


def _detect_slow_tests(output: str) -> list[str]:
    """Return the `has been running for over N seconds` warning lines, if any."""
    return [
        line.rstrip()
        for line in output.splitlines()
        if SLOW_TEST_RE.match(line)
    ]


def _git_status_porcelain() -> str:
    result = subprocess.run(
        ["git", "status", "--porcelain"],
        cwd=REPO_ROOT,
        capture_output=True,
        text=True,
        check=True,
    )
    # Ignore the port-loop script itself: agents may modify it mid-run and
    # the loop commits it separately, so a dirty script shouldn't block gates.
    script_rel = str(SCRIPT_PATH.relative_to(REPO_ROOT))
    lines = [
        ln for ln in result.stdout.splitlines(keepends=True)
        if ln[3:].rstrip() != script_rel
    ]
    return "".join(lines)


def is_tree_clean() -> bool:
    return not _git_status_porcelain().strip()


def apply_auto_fixes() -> None:
    """Run `just format` and `cargo clippy --fix` before the lint gate.

    If the working tree was clean before this ran and the auto-fixers produce
    changes, commit them here so the model is never dispatched for purely
    cosmetic lint output. If the tree was already dirty, leave the changes
    for the clean-tree gate / the model to handle as part of the dirty diff.
    """
    was_clean = is_tree_clean()
    run_gate(["just", "format"])
    run_gate(
        [
            "cargo",
            "clippy",
            "--all-features",
            "--tests",
            "--fix",
            "--allow-dirty",
            "--allow-staged",
        ]
    )
    run_gate(
        [
            "cargo",
            "clippy",
            "--manifest-path",
            "tests/conformance/rust/Cargo.toml",
            "--fix",
            "--allow-dirty",
            "--allow-staged",
        ]
    )
    if not was_clean or is_tree_clean():
        return
    print("\n[port-loop] auto-fixes produced changes; auto-committing.", flush=True)
    # -u stages only modifications to tracked files; format + clippy --fix
    # should never create new tracked files, and we don't want to sweep in
    # stray untracked files either.
    subprocess.run(["git", "add", "-u"], cwd=REPO_ROOT, check=True)
    subprocess.run(
        [
            "git",
            "commit",
            "-m",
            "Auto-apply `just format` + `cargo clippy --fix`",
        ],
        cwd=REPO_ROOT,
        check=True,
    )


def cargo_clean() -> None:
    """Wipe the target/ directory so each iteration starts from cold."""
    run_gate(["cargo", "clean"])


# ---- gate cache -------------------------------------------------------------
#
# Each gate records the HEAD SHA at which it last succeeded, but only when the
# working tree was clean at that moment (so the SHA fully captures the state).
# On the next run, if the tree is clean AND the cached SHA matches the current
# HEAD, we skip the gate. Any commit or uncommitted change invalidates the
# cache naturally. Failures are NOT cached — re-running them produces fresh
# output to hand to the fixer agent.

GATE_CACHE_PATH = REPO_ROOT / ".port-loop-cache.json"


def _load_gate_cache() -> dict[str, str]:
    if not GATE_CACHE_PATH.exists():
        return {}
    try:
        data = json.loads(GATE_CACHE_PATH.read_text())
    except (json.JSONDecodeError, OSError):
        return {}
    return data if isinstance(data, dict) else {}


def _save_gate_cache(cache: dict[str, str]) -> None:
    GATE_CACHE_PATH.write_text(json.dumps(cache, indent=2, sort_keys=True))


def _gate_cached(key: str) -> bool:
    """True if this gate already succeeded at the current HEAD on a clean tree."""
    if not is_tree_clean():
        return False
    return _load_gate_cache().get(key) == git_head()


def _record_gate_success(key: str) -> None:
    """Pin a gate's success to the current HEAD, but only if the tree is clean."""
    if not is_tree_clean():
        return
    cache = _load_gate_cache()
    cache[key] = git_head()
    _save_gate_cache(cache)


def _run_cached_gate(
    key: str,
    cmd: list[str],
    *,
    env: dict[str, str] | None = None,
    timeout: float | None = None,
    detect_slow: bool = False,
) -> tuple[bool, str, str | None]:
    """Run `cmd` (or return a cached pass), returning (ok, output, perf).

    `perf` is `"timeout"` if the gate was killed by the port-loop timer,
    `"slow"` if `detect_slow` is set and libtest emitted any "has been
    running for over N seconds" warnings (even when the exit code is
    clean), or `None` otherwise. A `"slow"` signal also demotes `ok`
    to `False` so the caller treats it as a failure.
    """
    if _gate_cached(key):
        print(f"\n[port-loop] gate cache hit: `{key}` @ HEAD; skipping.")
        return True, "", None
    code, out, perf = run_gate(cmd, env=env, timeout=timeout)
    ok = code == 0
    if ok and detect_slow:
        slow = _detect_slow_tests(out)
        if slow:
            print(
                f"\n[port-loop] {len(slow)} slow-test warning(s) despite "
                f"clean exit; treating gate as failed:"
            )
            for line in slow:
                print(f"  {line}")
            perf = "slow"
            ok = False
    if ok:
        _record_gate_success(key)
    return ok, out, perf


# Suite-wide `cargo test` runs occasionally legitimately take minutes;
# per-module runs should finish in well under a minute. Timeouts above
# those budgets mean something is pathological — we kill and hand the
# output to an agent with a perf-focused prompt.
SUITE_TEST_TIMEOUT_S = 15 * 60
MODULE_TEST_TIMEOUT_S = 2 * 60


def gate_lint() -> tuple[bool, str, str | None]:
    return _run_cached_gate("just lint", ["just", "lint"])


def gate_server_tests() -> tuple[bool, str, str | None]:
    return _run_cached_gate(
        "cargo test",
        ["cargo", "test"],
        timeout=SUITE_TEST_TIMEOUT_S,
        detect_slow=True,
    )


def gate_native_tests() -> tuple[bool, str, str | None]:
    env = os.environ.copy()
    env["HEGEL_SERVER_COMMAND"] = "/bin/false"
    return _run_cached_gate(
        "HEGEL_SERVER_COMMAND=/bin/false cargo test --features native",
        ["cargo", "test", "--features", "native"],
        env=env,
        timeout=SUITE_TEST_TIMEOUT_S,
        detect_slow=True,
    )


def gate_module_server(kind: str, module: str) -> tuple[bool, str, str | None]:
    return _run_cached_gate(
        f"cargo test --test {kind} {module}",
        ["cargo", "test", "--test", kind, module],
        timeout=MODULE_TEST_TIMEOUT_S,
        detect_slow=True,
    )


def gate_module_native(kind: str, module: str) -> tuple[bool, str, str | None]:
    env = os.environ.copy()
    env["HEGEL_SERVER_COMMAND"] = "/bin/false"
    return _run_cached_gate(
        f"HEGEL_SERVER_COMMAND=/bin/false cargo test --features native "
        f"--test {kind} {module}",
        ["cargo", "test", "--features", "native", "--test", kind, module],
        env=env,
        timeout=MODULE_TEST_TIMEOUT_S,
        detect_slow=True,
    )


def test_fix_prompt_for(perf: str | None, default_prompt: str) -> str:
    """Pick the right fix prompt for a test gate's perf signal."""
    if perf == "timeout":
        return TEST_PERF_FIX_PROMPT
    if perf == "slow":
        return TEST_SLOW_FIX_PROMPT
    return default_prompt


def destination_has_tests(dest: Path) -> bool:
    if not dest.exists():
        return False
    try:
        return "#[test]" in dest.read_text()
    except OSError:
        return False


def gate_clean_tree() -> tuple[bool, str]:
    print("\n$ git status --porcelain")
    out = _git_status_porcelain()
    if out:
        sys.stdout.write(out)
        sys.stdout.flush()
    return not out.strip(), out


def git_head() -> str:
    result = subprocess.run(
        ["git", "rev-parse", "HEAD"],
        cwd=REPO_ROOT,
        capture_output=True,
        text=True,
        check=True,
    )
    return result.stdout.strip()


def git_log(range_spec: str) -> str:
    result = subprocess.run(
        ["git", "log", range_spec, "--oneline", "--no-decorate"],
        cwd=REPO_ROOT,
        capture_output=True,
        text=True,
        check=True,
    )
    return result.stdout


# ---- unported-pool computation ----------------------------------------------


_SKIP_BULLET = re.compile(r"`(test_[\w_]+\.py)`")


def read_skipped(kind: str) -> set[str]:
    """Parse SKIPPED.md for the set of skipped filenames under `## <kind>`."""
    md = REPO_ROOT / "SKIPPED.md"
    if not md.exists():
        return set()
    sections: dict[str, list[str]] = {}
    current: str | None = None
    for line in md.read_text().splitlines():
        header = re.match(r"^\s*##\s+(\w+)", line)
        if header:
            current = header.group(1).lower()
            sections[current] = []
        elif current is not None:
            sections[current].append(line)
    body = "\n".join(sections.get(kind.lower(), []))
    return set(_SKIP_BULLET.findall(body))


def ported_stems(kind: str) -> set[str]:
    """Stems that already have a Rust port."""
    dirs = [REPO_ROOT / "tests" / kind]
    if kind == "pbtkit":
        dirs += [
            REPO_ROOT / "tests" / "test_shrink_quality",
            REPO_ROOT / "tests" / "test_find_quality",
            REPO_ROOT / "tests" / "embedded" / "native",
        ]
    stems: set[str] = set()
    for d in dirs:
        if not d.exists():
            continue
        for p in d.rglob("*.rs"):
            if p.name == "main.rs":
                continue
            stems.add(p.stem)
            if p.stem.endswith("_tests"):
                stems.add(p.stem[: -len("_tests")])
    return stems


def upstream_files(kind: str) -> list[Path]:
    root = {"pbtkit": PBTKIT_DIR, "hypothesis": HYPOTHESIS_DIR}[kind]
    if not root.exists():
        return []
    return sorted(root.rglob("test_*.py"))


def destination_for(upstream: Path) -> Path:
    """Map an upstream test path to its Rust port path.

    - `resources/pbtkit/tests/test_text.py` → `tests/pbtkit/text.rs`
    - `resources/pbtkit/tests/findability/test_types.py` →
      `tests/pbtkit/findability_types.rs`
    - `resources/hypothesis/.../cover/test_floats.py` →
      `tests/hypothesis/floats.rs`
    """
    if upstream.is_relative_to(PBTKIT_DIR):
        kind = "pbtkit"
        rel = upstream.relative_to(PBTKIT_DIR)
    else:
        kind = "hypothesis"
        rel = upstream.relative_to(HYPOTHESIS_DIR)
    stem = upstream.stem.removeprefix("test_")
    parts = list(rel.parent.parts) + [stem]
    return Path("tests") / kind / ("_".join(parts) + ".rs")


def unported_pool() -> list[Path]:
    pool: list[Path] = []
    for kind in ("pbtkit", "hypothesis"):
        skipped = read_skipped(kind)
        ported = ported_stems(kind)
        for path in upstream_files(kind):
            if path.name in skipped:
                continue
            # Check both the destination stem (which includes the subdirectory
            # prefix for pbtkit, e.g. "findability_pbtsmith_regressions") and
            # the plain stem (e.g. "pbtsmith_regressions"), because hypothesis
            # ports were created without subdirectory prefixes while pbtkit
            # ports include them.
            dest_stem = destination_for(path).stem
            plain_stem = path.stem.removeprefix("test_")
            if dest_stem in ported or plain_stem in ported:
                continue
            pool.append(path)
    return pool


def finalized_stems() -> set[str]:
    """Stems already integrated into the main test suite (recorded in FINALIZED.md)."""
    if not FINALIZED_MD.exists():
        return set()
    stems: set[str] = set()
    for line in FINALIZED_MD.read_text().splitlines():
        line = line.strip()
        if line.startswith("- "):
            stem = line[2:].split(" — ")[0].strip()
            if stem:
                stems.add(stem)
    return stems


def finalize_pool() -> list[Path]:
    """Files in tests/{hypothesis,pbtkit}/ not yet integrated into the main suite."""
    done = finalized_stems()
    all_files: list[Path] = []
    for kind in ("hypothesis", "pbtkit"):
        d = REPO_ROOT / "tests" / kind
        if d.exists():
            all_files.extend(
                sorted(p for p in d.glob("*.rs") if p.name != "main.rs")
            )
    return [f for f in all_files if f.stem not in done]


# ---- TODO handling ----------------------------------------------------------


TODO_PATH = REPO_ROOT / "TODO.yaml"


def read_todos() -> list[dict]:
    """Parse TODO.yaml into a list of entry dicts. Empty/missing file → []."""
    if not TODO_PATH.exists():
        return []
    data = yaml.safe_load(TODO_PATH.read_text())
    if data is None:
        return []
    if not isinstance(data, list):
        sys.exit(
            f"[port-loop] TODO.yaml must be a YAML list at the top level, "
            f"got {type(data).__name__}"
        )
    return data


def format_todo(entry: dict) -> str:
    """Render one TODO entry as a YAML fragment for inclusion in a prompt."""
    return yaml.safe_dump([entry], sort_keys=False).rstrip()


def _todo_hash(entry: dict) -> str:
    """Content hash of a TODO entry (short sha1 of its YAML fragment).

    Used as a stable key for `_load_todo_attempts` so that rewriting an
    entry's text resets its attempt counter — the agent has produced a
    visibly different entry and deserves a fresh budget.
    """
    return hashlib.sha1(format_todo(entry).encode("utf-8")).hexdigest()[:16]


TODO_ATTEMPTS_PATH = REPO_ROOT / ".port-loop-todo-attempts.json"


def _load_todo_attempts() -> dict[str, int]:
    """Load the persisted `{content_hash: attempt_count}` map."""
    if not TODO_ATTEMPTS_PATH.exists():
        return {}
    try:
        data = json.loads(TODO_ATTEMPTS_PATH.read_text())
    except (json.JSONDecodeError, OSError):
        return {}
    if not isinstance(data, dict):
        return {}
    return {str(k): int(v) for k, v in data.items() if isinstance(v, int)}


def _save_todo_attempts(attempts: dict[str, int]) -> None:
    TODO_ATTEMPTS_PATH.write_text(
        json.dumps(attempts, indent=2, sort_keys=True)
    )


def _prune_todo_attempts(
    attempts: dict[str, int], live_hashes: set[str]
) -> dict[str, int]:
    """Drop attempt counters for hashes that are no longer in TODO.yaml."""
    return {h: n for h, n in attempts.items() if h in live_hashes}


# ---- claude dispatch ---------------------------------------------------------


def _tool_summary(name: str, inp: dict) -> str:
    """Render a one-line summary of a tool use for live logging."""
    if not isinstance(inp, dict):
        return ""
    if name == "Bash":
        cmd = str(inp.get("command", "")).strip().splitlines()
        return cmd[0][:200] if cmd else ""
    if name in ("Read", "Write", "Edit", "NotebookEdit"):
        return str(inp.get("file_path", ""))
    if name == "Glob":
        parts = [str(inp.get("pattern", ""))]
        if inp.get("path"):
            parts.append(f"in {inp['path']}")
        return " ".join(parts)
    if name == "Grep":
        parts = [repr(str(inp.get("pattern", "")))]
        if inp.get("path"):
            parts.append(f"in {inp['path']}")
        if inp.get("glob"):
            parts.append(f"glob={inp['glob']}")
        return " ".join(parts)
    if name == "TodoWrite":
        todos = inp.get("todos") or []
        return f"{len(todos)} todos"
    if name == "Task":
        return str(inp.get("description", ""))[:200]
    # Generic fallback: trimmed JSON.
    blob = json.dumps(inp, default=str)
    return blob[:200] + ("…" if len(blob) > 200 else "")


def _print_tool_detail(name: str, inp: dict) -> None:
    """Print additional indented context below the tool-use header.

    For file-modifying tools, show the diff / content so the user can
    follow along without opening the file. Truncate aggressively to
    keep the live log readable; claude's own output already captures
    the full content.
    """
    if not isinstance(inp, dict):
        return
    max_lines = 40
    if name == "Edit":
        old_s = str(inp.get("old_string", ""))
        new_s = str(inp.get("new_string", ""))
        if inp.get("replace_all"):
            print("[claude]     (replace_all=True)", flush=True)
        diff = list(
            difflib.unified_diff(
                old_s.splitlines(),
                new_s.splitlines(),
                fromfile="old",
                tofile="new",
                lineterm="",
                n=1,
            )
        )
        # Skip the `--- old` / `+++ new` header lines — they're noise
        # when there's only ever one hunk per Edit call.
        body = [line for line in diff if not line.startswith(("---", "+++"))]
        for line in body[:max_lines]:
            print(f"[claude]     {line}", flush=True)
        if len(body) > max_lines:
            print(
                f"[claude]     … ({len(body) - max_lines} more diff lines)",
                flush=True,
            )
        return
    if name == "Write":
        content = str(inp.get("content", ""))
        lines = content.splitlines()
        for line in lines[:max_lines]:
            print(f"[claude]     + {line}", flush=True)
        if len(lines) > max_lines:
            print(
                f"[claude]     … ({len(lines) - max_lines} more lines)",
                flush=True,
            )
        return
    if name == "NotebookEdit":
        new_source = str(inp.get("new_source", ""))
        lines = new_source.splitlines()
        mode = inp.get("edit_mode") or "replace"
        print(f"[claude]     (mode={mode})", flush=True)
        for line in lines[:max_lines]:
            print(f"[claude]     + {line}", flush=True)
        if len(lines) > max_lines:
            print(
                f"[claude]     … ({len(lines) - max_lines} more lines)",
                flush=True,
            )
        return


def _print_event(evt: dict) -> None:
    """Print a human-friendly line for one stream-json event."""
    etype = evt.get("type")
    if etype == "system" and evt.get("subtype") == "init":
        sid = evt.get("session_id", "?")
        cwd = evt.get("cwd", "?")
        print(f"[claude] init session={sid} cwd={cwd}", flush=True)
        return
    if etype == "assistant":
        for block in (evt.get("message") or {}).get("content", []) or []:
            btype = block.get("type")
            if btype == "text":
                for line in (block.get("text") or "").splitlines():
                    if line.strip():
                        print(f"[claude] {line}", flush=True)
            elif btype == "tool_use":
                name = block.get("name", "?")
                inp = block.get("input") or {}
                summary = _tool_summary(name, inp)
                print(f"[claude] → {name}({summary})", flush=True)
                _print_tool_detail(name, inp)
            elif btype == "thinking":
                for line in (block.get("thinking") or "").splitlines():
                    if line.strip():
                        print(f"[claude:think] {line}", flush=True)
        return
    if etype == "user":
        for block in (evt.get("message") or {}).get("content", []) or []:
            if block.get("type") != "tool_result":
                continue
            if block.get("is_error"):
                content = block.get("content")
                text = content if isinstance(content, str) else json.dumps(content)
                first = (text or "").strip().splitlines()[:1]
                print(
                    f"[claude] ← ERROR: {first[0] if first else ''}", flush=True
                )
        return
    if etype == "result":
        subtype = evt.get("subtype", "")
        turns = evt.get("num_turns")
        cost = evt.get("total_cost_usd")
        duration_ms = evt.get("duration_ms")
        pieces = [f"result={subtype}"]
        if turns is not None:
            pieces.append(f"turns={turns}")
        if duration_ms is not None:
            pieces.append(f"{duration_ms / 1000:.1f}s")
        if cost is not None:
            pieces.append(f"${cost:.4f}")
        print(f"[claude] {' '.join(pieces)}", flush=True)
        res = evt.get("result")
        if isinstance(res, str) and res.strip():
            first = res.strip().splitlines()[0]
            print(f"[claude] final: {first[:300]}", flush=True)
        return


def dispatch_claude(
    prompt: str,
    *,
    gate_output: str | None,
    timeout: float | None,
    model: str,
    max_budget_usd: float | None = None,
    resume_session: str | None = None,
    skip_permissions: bool = False,
    cwd_override: Path | None = None,
) -> tuple[str | None, int]:
    """Spawn claude -p (or resume an existing session), stream events live.

    Returns `(session_id, exit_code)`. `session_id` is the id reported in
    the `system/init` event (None if the stream never produced one, e.g.
    the process failed before handshake). `exit_code` is the subprocess
    return code; non-zero + resume = the resume target is likely stale
    and the caller should drop it.
    """
    full_prompt = prompt
    if gate_output is not None:
        full_prompt += f"\n\nGate output:\n{gate_output}"
    print("\n" + "=" * 72, flush=True)
    if resume_session is not None:
        print(
            f"Resuming claude session {resume_session[:12]}… "
            f"(model={model}) with prompt:",
            flush=True,
        )
    else:
        print(f"Dispatching fresh claude (model={model}) with prompt:", flush=True)
    print("-" * 72, flush=True)
    print(full_prompt, flush=True)
    print("=" * 72, flush=True)

    cmd = [
        "claude",
        "-p",
        "--model",
        model,
        "--output-format",
        "stream-json",
        "--verbose",
    ]
    if skip_permissions:
        cmd.append("--dangerously-skip-permissions")
    # Worker worktrees don't inherit the supervisor's .claude/settings.local.json
    # (it's gitignored, so `git worktree add` skips it). Pass the skills /
    # commands / agents allowlist directly on the CLI so every dispatched
    # claude — in any cwd — can edit its own skill/agent/command files
    # without prompting, regardless of which settings files it finds.
    # `--allowedTools` is variadic; the `--flag=value` form is required
    # so argparse doesn't swallow the positional prompt into the list.
    cmd.append(
        "--allowedTools="
        "Edit(.claude/skills/**),"
        "Write(.claude/skills/**),"
        "Edit(.claude/commands/**),"
        "Write(.claude/commands/**),"
        "Edit(.claude/agents/**),"
        "Write(.claude/agents/**)"
    )
    if max_budget_usd is not None:
        cmd += ["--max-budget-usd", str(max_budget_usd)]
    if resume_session is not None:
        # Resuming carries forward the original session's system prompt
        # and history, so don't re-append. The follow-up task goes in as
        # the final positional prompt argument.
        cmd += ["--resume", resume_session]
    else:
        cmd += ["--append-system-prompt", COMMON_SYSTEM_PROMPT]
    cmd.append(full_prompt)

    proc_cwd = cwd_override if cwd_override is not None else REPO_ROOT
    proc = subprocess.Popen(
        cmd,
        cwd=str(proc_cwd),
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
        bufsize=1,
    )

    timed_out = False

    def _kill_on_timeout() -> None:
        nonlocal timed_out
        timed_out = True
        try:
            proc.kill()
        except Exception:
            pass

    timer = (
        threading.Timer(timeout, _kill_on_timeout) if timeout is not None else None
    )
    if timer is not None:
        timer.daemon = True
        timer.start()

    sessions_dir = (
        (cwd_override / ".porting" / "sessions")
        if cwd_override is not None
        else SESSIONS_DIR
    )
    sessions_dir.mkdir(parents=True, exist_ok=True)
    if resume_session is not None:
        log_path = sessions_dir / f"{resume_session}.jsonl"
        pending_path = None
    else:
        pending_path = (
            sessions_dir / f".pending-{os.getpid()}-{int(time.time())}.jsonl"
        )
        log_path = pending_path
    log_file = log_path.open("a", encoding="utf-8")
    print(f"[port-loop] logging raw stream to {log_path}", flush=True)

    session_id: str | None = None
    assert proc.stdout is not None
    try:
        for raw in proc.stdout:
            line = raw.rstrip("\n")
            if not line:
                continue
            log_file.write(line + "\n")
            log_file.flush()
            try:
                evt = json.loads(line)
            except json.JSONDecodeError:
                print(f"[claude:raw] {line}", flush=True)
                continue
            if evt.get("type") == "system" and evt.get("subtype") == "init":
                sid = evt.get("session_id")
                if isinstance(sid, str) and session_id is None:
                    session_id = sid
                    if pending_path is not None:
                        final_path = sessions_dir / f"{sid}.jsonl"
                        log_file.close()
                        # Merge into any pre-existing log for this session id
                        # (shouldn't happen for a fresh session, but be safe).
                        if final_path.exists():
                            with (
                                pending_path.open("r", encoding="utf-8") as src,
                                final_path.open("a", encoding="utf-8") as dst,
                            ):
                                dst.write(src.read())
                            pending_path.unlink()
                        else:
                            pending_path.rename(final_path)
                        log_path = final_path
                        pending_path = None
                        log_file = log_path.open("a", encoding="utf-8")
            try:
                _print_event(evt)
            except Exception as e:
                print(f"[port-loop] event-format error: {e}", flush=True)
    finally:
        if timer is not None:
            timer.cancel()
        proc.wait()
        log_file.close()
        if timed_out:
            print(f"\n[port-loop] claude timed out after {timeout}s; continuing.")

    return session_id, proc.returncode


# ---- main loop ---------------------------------------------------------------


class GateBudgetExhausted(Exception):
    """Raised when a per-file dispatch cap is hit inside a worker.

    Caught by `main()` in `--worker-mode` to exit with code 42, signalling
    to the supervisor that a rescue dispatch is warranted.
    """

    def __init__(self, file: str, cap: int) -> None:
        super().__init__(
            f"per-file dispatch budget of {cap} exhausted while porting {file}"
        )
        self.file = file
        self.cap = cap


class IterCounter:
    """Tracks and caps total claude dispatches across outer and sub-loops.

    Also remembers the session_id of the most recent dispatch so that
    follow-up prompts ("commit the dirty tree you just produced") can
    `--resume` that same session instead of spawning a context-free
    fresh agent that would have to re-derive the diff.
    """

    def __init__(
        self,
        max_iterations: int,
        timeout: float | None,
        model: str,
        max_budget_usd: float | None = None,
        skip_permissions: bool = False,
    ) -> None:
        self.n = 0
        self.max = max_iterations
        self.timeout = timeout
        self.model = model
        self.max_budget_usd = max_budget_usd
        self.skip_permissions = skip_permissions
        self.last_session_id: str | None = None
        # Per-file dispatch cap (only set in worker mode). `per_file_n`
        # counts dispatches since the last `reset_per_file()` — drive_port
        # is the only consumer. When the cap is hit we raise
        # `GateBudgetExhausted` rather than `sys.exit(0)`, so the worker
        # can clean up and emit an exit code the supervisor understands.
        self.per_file_cap: int | None = None
        self.per_file_n = 0
        self.per_file_label: str | None = None

    def reset_per_file(self, label: str) -> None:
        """Start fresh per-file counter. Call at the top of each port target."""
        self.per_file_n = 0
        self.per_file_label = label

    def _check_cap(self) -> None:
        if self.per_file_cap is not None and self.per_file_n >= self.per_file_cap:
            raise GateBudgetExhausted(
                self.per_file_label or "<unknown>", self.per_file_cap
            )
        if self.max > 0 and self.n >= self.max:
            print(f"\n[port-loop] hit --max-iterations={self.max}; stopping.")
            sys.exit(0)

    def dispatch(self, prompt: str, *, gate_output: str | None = None) -> None:
        """Dispatch a fresh claude session, or exit 0 if the cap is hit."""
        self._check_cap()
        self.n += 1
        self.per_file_n += 1
        print(f"\n{'#' * 72}\n# iteration {self.n}\n{'#' * 72}")
        sid, _code = dispatch_claude(
            prompt,
            gate_output=gate_output,
            timeout=self.timeout,
            model=self.model,
            max_budget_usd=self.max_budget_usd,
            skip_permissions=self.skip_permissions,
        )
        # Even if exit code was non-zero, a captured sid still means a
        # valid session exists that we can try to resume later.
        self.last_session_id = sid

    def resume_last(
        self, prompt: str, *, gate_output: str | None = None
    ) -> None:
        """Resume the most recent dispatched session with a follow-up.

        If there is no prior session to resume (fresh script run with a
        pre-existing dirty tree), falls back to a fresh dispatch —
        that agent has no context either way and will have to figure
        it out from the diff.

        If the resume subprocess exits non-zero, drops `last_session_id`
        so the next call won't retry the same (likely stale) target.
        """
        if self.last_session_id is None:
            self.dispatch(prompt, gate_output=gate_output)
            return
        self._check_cap()
        self.n += 1
        self.per_file_n += 1
        previous = self.last_session_id
        print(
            f"\n{'#' * 72}\n# iteration {self.n} "
            f"(resume {previous[:12]}…)\n{'#' * 72}"
        )
        sid, code = dispatch_claude(
            prompt,
            gate_output=gate_output,
            timeout=self.timeout,
            model=self.model,
            max_budget_usd=self.max_budget_usd,
            resume_session=previous,
            skip_permissions=self.skip_permissions,
        )
        if code != 0:
            print(
                f"\n[port-loop] resume of {previous[:12]} exited {code}; "
                f"dropping session id so the next call starts fresh."
            )
            self.last_session_id = None
            return
        if sid is not None:
            self.last_session_id = sid


def drive_port(picked: Path, destination: Path, state: IterCounter) -> None:
    """Sub-loop driving one port; exits via `state.dispatch` if the cap is hit."""
    kind = "pbtkit" if picked.is_relative_to(PBTKIT_DIR) else "hypothesis"
    module = destination.stem
    start_sha = git_head()
    fmt_args = dict(
        path=picked,
        destination=destination,
        name=picked.name,
        kind=kind,
        module=module,
    )
    print(
        f"\n[port-loop] entering sub-loop for {picked} → {destination} "
        f"(module '{module}' in test binary '{kind}'); start sha "
        f"{start_sha[:12]}."
    )
    while True:
        # Exit A: upstream is now in SKIPPED.md. Checked at the top of every
        # iteration so that any dispatch below which causes the agent to add
        # the file to SKIPPED.md short-circuits on the next turn rather than
        # triggering a later step (e.g., PORT_MISSING_TESTS_PROMPT firing on
        # a non-existent destination after the agent chose to skip).
        if picked.name in read_skipped(kind):
            # Step 5: tree must be clean.
            ok, out = gate_clean_tree()
            if ok:
                print(
                    f"\n[port-loop] {picked.name} is in SKIPPED.md; sub-loop done."
                )
                return
            else:
                state.resume_last(
                    PORT_COMMIT_PROMPT.format(**fmt_args), gate_output=out
                )
                continue

        # Each step below `continue`s after dispatching, so the skip check
        # above re-runs before any later step fires.

        # Step 1: destination must exist.
        if not destination.exists():
            state.dispatch(PORT_PROMPT.format(**fmt_args))
            continue

        # Step 2: destination must contain at least one #[test].
        if not destination_has_tests(destination):
            state.dispatch(PORT_MISSING_TESTS_PROMPT.format(**fmt_args))
            continue

        # Step 3: module's server-mode tests must pass.
        ok, out, perf = gate_module_server(kind, module)
        if not ok:
            prompt = test_fix_prompt_for(
                perf, PORT_TEST_FIX_SERVER_PROMPT.format(**fmt_args)
            )
            state.dispatch(prompt, gate_output=strip_build_noise(out))
            continue

        # Step 4: module's native-mode tests must pass.
        ok, out, perf = gate_module_native(kind, module)
        if not ok:
            prompt = test_fix_prompt_for(
                perf, PORT_TEST_FIX_NATIVE_PROMPT.format(**fmt_args)
            )
            state.dispatch(prompt, gate_output=strip_build_noise(out))
            continue

        # Step 5: tree must be clean.
        ok, out = gate_clean_tree()
        if not ok:
            state.resume_last(
                PORT_COMMIT_PROMPT.format(**fmt_args), gate_output=out
            )
            continue

        # All gates passed. If nothing was committed during this sub-loop
        # there's nothing to review or reflect on.
        current_sha = git_head()
        if current_sha == start_sha:
            print(
                f"\n[port-loop] {destination} green with no new commits; "
                f"sub-loop done."
            )
            break

        # Step 6: skill-update reflection pass. Ask the agent to capture
        # anything this port taught us into `.claude/skills/`, possibly
        # creating new skills, before the reviewer sees the port. Skill
        # edits are markdown, so they can't break code gates — we don't
        # loop back to re-verify after this step; we just make sure any
        # uncommitted skill changes get committed, then fall through to
        # the review.
        log = git_log(f"{start_sha}..HEAD")
        print(
            f"\n[port-loop] {destination} ported and green; "
            f"dispatching skill-update reflection of {start_sha[:12]}"
            f"..HEAD."
        )
        state.dispatch(
            PORT_SKILL_UPDATE_PROMPT.format(
                start_sha=start_sha, log=log, **fmt_args
            )
        )
        ok, out = gate_clean_tree()
        if not ok:
            state.resume_last(COMMIT_PROMPT, gate_output=out)

        # Step 7: dispatch a review of the commits made during this port
        # (including any skill-update commits from the previous step).
        current_sha = git_head()
        log = git_log(f"{start_sha}..HEAD")
        print(
            f"\n[port-loop] dispatching review of {start_sha[:12]}..HEAD."
        )
        state.dispatch(
            PORT_REVIEW_PROMPT.format(start_sha=start_sha, log=log, **fmt_args)
        )
        if git_head() == current_sha:
            print(
                f"\n[port-loop] review made no changes; sub-loop done."
            )
            break
        print(
            f"\n[port-loop] review made changes; re-verifying gates."
        )
        # Loop around to re-run the gates on the reviewer's changes.


def drive_finalize_file(picked: Path, state: IterCounter) -> None:
    """Sub-loop integrating one ported test file into the main test suite.

    Dispatches agents until the file no longer exists and its stem appears
    in FINALIZED.md, running full server + native test gates each round.
    """
    rel = picked.relative_to(REPO_ROOT)
    kind = "hypothesis" if "hypothesis" in picked.parts else "pbtkit"
    print(f"\n[port-loop] finalize: sub-loop for {rel}")

    while True:
        # Done when file is gone and stem recorded in FINALIZED.md.
        if not picked.exists() and picked.stem in finalized_stems():
            print(
                f"\n[port-loop] finalize: {picked.stem} integrated; "
                f"sub-loop done."
            )
            return

        # File removed but not yet recorded — nudge agent to write the entry.
        if not picked.exists():
            state.resume_last(
                FINALIZE_RECORD_PROMPT.format(path=rel, stem=picked.stem)
            )
        else:
            state.dispatch(
                FINALIZE_PROMPT.format(path=rel, stem=picked.stem, kind=kind)
            )

        # Gate: full server tests.
        ok, out, _ = gate_server_tests()
        if not ok:
            state.dispatch(
                FINALIZE_FIX_PROMPT.format(path=rel),
                gate_output=strip_build_noise(out),
            )
            continue

        # Gate: full native tests.
        ok, out, _ = gate_native_tests()
        if not ok:
            state.dispatch(
                FINALIZE_FIX_PROMPT.format(path=rel),
                gate_output=strip_build_noise(out),
            )
            continue

        # Gate: clean tree.
        ok, out = gate_clean_tree()
        if not ok:
            state.resume_last(COMMIT_PROMPT, gate_output=out)


def _rebase_in_progress() -> bool:
    """True if `.git/rebase-{merge,apply}/` exists in REPO_ROOT's gitdir."""
    r = _run_capture(["git", "rev-parse", "--git-path", "rebase-merge"], cwd=REPO_ROOT)
    if r.returncode == 0 and Path(r.stdout.strip()).exists():
        return True
    r = _run_capture(["git", "rev-parse", "--git-path", "rebase-apply"], cwd=REPO_ROOT)
    if r.returncode == 0 and Path(r.stdout.strip()).exists():
        return True
    return False


def _gates_green_for(kind: str, module: str) -> tuple[bool, str]:
    """Fast re-check of the module's server+native gates + clean tree.

    Used after a worker-side rebase to decide whether the port survived.
    Returns `(ok, combined_output)`. Skips slow-test detection so a
    transient perf signal doesn't flip a passing rebase into a fail.
    """
    ok, out, _ = gate_module_server(kind, module)
    if not ok:
        return False, out
    ok, out, _ = gate_module_native(kind, module)
    if not ok:
        return False, out
    ok, out = gate_clean_tree()
    if not ok:
        return False, out
    return True, ""


def post_rebase(
    picked: Path, destination: Path, supervisor_branch: str, state: IterCounter
) -> None:
    """Fetch, rebase worker branch onto origin/<supervisor_branch>, re-verify.

    Worker-side step run after `drive_port` goes green. The rebase may
    pull in changes from the supervisor branch (or from other workers'
    commits already merged there) that regress the port; if so, we
    dispatch one fix pass and re-check. A second failure escalates to
    `GateBudgetExhausted` (rescue in the supervisor).

    Raises `RuntimeError` on unrecoverable rebase conflicts so the worker
    exits 43 and the supervisor dispatches a merge-rescue agent.
    """
    kind = "pbtkit" if picked.is_relative_to(PBTKIT_DIR) else "hypothesis"
    module = destination.stem

    fetch = _run_capture(["git", "fetch", "origin"], cwd=REPO_ROOT)
    if fetch.returncode != 0:
        raise RuntimeError(
            f"post-rebase fetch failed:\n{fetch.stdout}\n{fetch.stderr}"
        )
    target = f"origin/{supervisor_branch}"
    target_sha = _rev_parse(target)
    if target_sha is None:
        print(
            f"\n[port-loop] post-rebase: {target} does not exist; "
            f"skipping rebase."
        )
        return
    if target_sha == git_head():
        print(
            f"\n[port-loop] post-rebase: already at {target_sha[:12]}; "
            f"no rebase needed."
        )
        return

    rebase = _run_capture(["git", "rebase", target], cwd=REPO_ROOT)
    if rebase.returncode != 0:
        # Don't abort. Let an agent resolve the conflict; a merge is
        # almost always recoverable (e.g. two workers both appending to
        # SKIPPED.md) and the existing gate-check loop catches the case
        # where resolution leaves the tree in a regressed state.
        print(
            "\n[port-loop] post-rebase: conflict during rebase; "
            "dispatching resolution agent."
        )
        status = _run_capture(["git", "status"], cwd=REPO_ROOT)
        prompt = POST_REBASE_CONFLICT_PROMPT.format(
            supervisor_branch=supervisor_branch,
        )
        state.dispatch(
            prompt,
            gate_output=rebase.stdout + rebase.stderr + "\n" + status.stdout,
        )
        if _rebase_in_progress():
            # Agent gave up or stalled. Abort so we leave a clean tree
            # and escalate to merge-rescue.
            _run_capture(["git", "rebase", "--abort"], cwd=REPO_ROOT)
            raise RuntimeError(
                f"post-rebase `git rebase {target}` had conflicts the "
                f"resolution agent could not finish."
            )
        print(
            f"\n[port-loop] post-rebase: conflict resolved; now at "
            f"{git_head()[:12]}."
        )
    else:
        print(f"\n[port-loop] post-rebase: rebased onto {target_sha[:12]}.")

    # Verify gates still pass. On failure, dispatch one fix; if that
    # doesn't restore green, raise so the worker hands off to rescue.
    for attempt in range(2):
        ok, out = _gates_green_for(kind, module)
        if ok:
            return
        if attempt == 1:
            raise RuntimeError(
                f"post-rebase gates still broken after one fix pass:\n{out}"
            )
        prompt = POST_REBASE_FIX_PROMPT.format(
            path=picked,
            destination=destination,
            supervisor_branch=supervisor_branch,
        )
        state.dispatch(prompt, gate_output=strip_build_noise(out))


def drive_port_worker(
    picked: Path,
    destination: Path,
    supervisor_branch: str,
    state: IterCounter,
) -> None:
    """Worker-side entrypoint: drive_port + post-rebase under a per-file cap.

    Raises `GateBudgetExhausted` if the budget is exhausted; the caller
    (`main()` in `--worker-mode`) translates that into exit code 42 so
    the supervisor can invoke the rescue agent. Post-rebase failures
    raise `RuntimeError`, translated to exit 43 for merge-rescue.
    """
    state.reset_per_file(str(picked))
    drive_port(picked, destination, state)
    post_rebase(picked, destination, supervisor_branch, state)


# ---- parallel-worker supervisor ---------------------------------------------
#
# `drive_port_pool` replaces the final `pick + drive_port` step in the main
# outer loop when `--max-workers > 1`. It owns admission gating (no new
# workers unless TODO.yaml is empty and PR CI isn't in a known-bad state),
# serial integration of worker commits via cherry-pick, rescue dispatches
# on gate-budget exhaustion, and clean shutdown on SIGINT. The outer loop
# reclaims control between pool cycles (so repair/sync/TODO/CI gates run
# unchanged), and exits the porting phase only after every worker in
# flight has been drained.


_POOL_ADMISSION_CACHE: dict = {"ts": 0.0, "result": None}
_POOL_ADMISSION_TTL = 30.0  # seconds
_SPAWN_STAGGER_SECS = 30.0  # minimum gap between successive worker spawns


def _pool_admission_ok() -> tuple[bool, str]:
    """Check whether new port workers may be spawned right now.

    Admission is denied only when PR CI has already-failing checks
    (either completed-failed or pending-with-fails). TODO.yaml entries
    no longer block admission — the dedicated TODO worker drains them
    in parallel. Returns `(ok, reason)` — the reason is useful for
    logging.

    Result is cached for `_POOL_ADMISSION_TTL` seconds to avoid hitting
    `gh api` on every 1 s pool tick; the outer loop re-evaluates
    admission between pool cycles anyway.
    """
    now = time.monotonic()
    cached = _POOL_ADMISSION_CACHE["result"]
    if cached is not None and now - _POOL_ADMISSION_CACHE["ts"] < _POOL_ADMISSION_TTL:
        return cached
    status, summary, _detail, failing = _pr_check_status(fetch_logs=False)
    if status == "failure":
        result = (False, f"PR CI failure: {summary}")
    elif failing > 0:
        result = (False, f"PR CI has {failing} failing check(s): {summary}")
    else:
        result = (True, status or "ok")
    _POOL_ADMISSION_CACHE["ts"] = now
    _POOL_ADMISSION_CACHE["result"] = result
    return result


def _pool_admission_invalidate() -> None:
    """Force the next `_pool_admission_ok` call to re-fetch (bypass cache)."""
    _POOL_ADMISSION_CACHE["result"] = None
    _POOL_ADMISSION_CACHE["ts"] = 0.0


def _dispatch_worker_rescue(
    slot: int,
    file: Path,
    dest: Path,
    per_file_cap: int,
    state: IterCounter,
) -> None:
    """Dispatch a rescue agent in worker `slot`'s worktree.

    The agent is asked to abandon the port cleanly — record a SKIPPED.md
    entry (and optionally a TODO entry) and make one focused commit.
    The commit lands on `port/worker-{slot}`, and the supervisor then
    cherry-picks it like any other worker commit.
    """
    worktree = _worker_path(slot)
    sessions = worktree / ".porting" / "sessions"
    tail = "(no worker session log available)"
    if sessions.exists():
        finalized = sorted(
            (p for p in sessions.glob("*.jsonl") if not p.name.startswith(".pending-")),
            key=lambda p: p.stat().st_mtime,
        )
        if finalized:
            sid = finalized[-1].stem
            tail = _session_tail(sid, max_lines=200)
            # `_session_tail` reads from supervisor's SESSIONS_DIR; for
            # worker worktrees we need to read from the worker's own
            # sessions dir, so override.
            try:
                lines = finalized[-1].read_text(
                    encoding="utf-8", errors="replace"
                ).splitlines()
                tail = "\n".join(lines[-200:])
            except OSError as e:
                tail = f"(could not read {finalized[-1]}: {e})"
    prompt = PORT_RESCUE_PROMPT.format(
        path=file,
        destination=dest,
        name=file.name,
        dispatches=per_file_cap,
        log_lines=200,
        session_tail=tail,
    )
    print(
        f"\n[port-loop] pool: dispatching rescue for worker {slot} "
        f"({file.name}) in {worktree}.",
        flush=True,
    )
    state._check_cap()
    state.n += 1
    sid, _code = dispatch_claude(
        prompt,
        gate_output=None,
        timeout=state.timeout,
        model=state.model,
        max_budget_usd=state.max_budget_usd,
        skip_permissions=state.skip_permissions,
        cwd_override=worktree,
    )
    # Don't carry the rescue session id into the supervisor's
    # `last_session_id` — it's tied to the worker's worktree, so a
    # `resume_last` from supervisor context would be wrong.


def _dispatch_merge_rescue(
    slot: int,
    file: Path,
    detail: str,
    state: IterCounter,
) -> None:
    """Abandon a worker's port when cherry-picking their commits conflicts.

    Runs in the supervisor's worktree. The agent is asked to add the
    file to SKIPPED.md with a one-line rationale citing the integration
    conflict, then commit. The worker's own branch is left untouched
    for possible human inspection.
    """
    prompt = (
        f"Parallel port-loop integration: cherry-picking "
        f"`{_worker_branch(slot)}`'s commits for the port of "
        f"`{file.name}` onto the supervisor branch failed. Abandon "
        f"the port: add `{file.name}` to the appropriate section of "
        f"SKIPPED.md (pbtkit or hypothesis) with a one-line rationale "
        f"citing the integration conflict, then make one focused "
        f"commit. Do NOT try to resolve the conflict or touch the "
        f"worker's branch — a later human can inspect it.\n\n"
        f"Integration failure detail:\n\n{detail}"
    )
    state.dispatch(prompt)


def _spawn_worker(
    slot: int,
    file: Path,
    base_sha: str,
    supervisor_branch: str,
    args: argparse.Namespace,
) -> subprocess.Popen:
    """Reset worker `slot`'s worktree to `base_sha` and spawn its subprocess."""
    _maybe_clean_worker_target(slot)
    worktree = _ensure_worktree(slot, base_sha)
    # Use the worktree's own copy of this script so `__file__`-relative
    # constants (REPO_ROOT, SESSIONS_DIR, etc.) resolve inside the
    # worktree rather than the supervisor's checkout. `git worktree add`
    # preserves the executable bit.
    script = worktree / "scripts" / "port-loop.py"
    if not script.exists():
        raise RuntimeError(
            f"worker worktree {worktree} is missing scripts/port-loop.py"
        )
    env = os.environ.copy()
    env["CARGO_TARGET_DIR"] = str(_worker_target_dir(slot))
    # Worker worktrees don't have the gitignored `resources/` upstream-test
    # tree; point the worker at the supervisor's copy so PBTKIT_DIR /
    # HYPOTHESIS_DIR and `destination_for` resolve correctly.
    env["PORT_LOOP_RESOURCES_BASE"] = str(REPO_ROOT)
    cmd = [
        str(script),
        "--worker-mode",
        "--worktree", str(worktree),
        "--port", str(file),
        "--supervisor-branch", supervisor_branch,
        "--log-prefix", f"worker-{slot}",
        "--per-file-dispatches", str(args.per_file_dispatches),
        "--model", args.model,
        "--timeout", str(args.timeout),
        "--max-iterations", str(args.max_iterations),
    ]
    if args.max_budget_usd:
        cmd += ["--max-budget-usd", str(args.max_budget_usd)]
    if args.skip_permissions:
        cmd.append("--dangerously-skip-permissions")
    print(
        f"\n[port-loop] pool: spawning worker {slot} for {file.name} "
        f"(base {base_sha[:12]}, worktree {worktree}).",
        flush=True,
    )
    return subprocess.Popen(cmd, env=env, start_new_session=True)


def _spawn_todo_worker(
    picked: tuple[dict, int, int, int, str],
    base_sha: str,
    supervisor_branch: str,
    args: argparse.Namespace,
) -> subprocess.Popen:
    """Reset the todo-worker worktree to `base_sha` and spawn it.

    `picked` is the tuple returned by `_pick_todo_for_dispatch` — entry,
    attempts, remaining, idx, hash. Only the first three are passed to
    the worker (idx/hash aren't needed once the entry is captured).
    """
    entry, attempts, remaining, _idx, _entry_hash = picked
    worktree = _ensure_todo_worktree(base_sha)
    script = worktree / "scripts" / "port-loop.py"
    if not script.exists():
        raise RuntimeError(
            f"todo-worker worktree {worktree} is missing scripts/port-loop.py"
        )
    # Stash the picked entry under .porting/ (gitignored) so writing it
    # doesn't dirty the worktree.
    payload_dir = worktree / ".porting"
    payload_dir.mkdir(parents=True, exist_ok=True)
    payload = payload_dir / "todo-payload.yaml"
    payload.write_text(
        yaml.safe_dump(
            {"entry": entry, "attempts": attempts, "remaining": remaining},
            sort_keys=False,
        )
    )
    env = os.environ.copy()
    env["CARGO_TARGET_DIR"] = str(_todo_worker_target_dir())
    env["PORT_LOOP_RESOURCES_BASE"] = str(REPO_ROOT)
    cmd = [
        str(script),
        "--todo-worker-mode",
        "--worktree", str(worktree),
        "--supervisor-branch", supervisor_branch,
        "--todo-payload", str(payload),
        "--log-prefix", "worker-todo",
        "--model", args.model,
        "--timeout", str(args.timeout),
        "--max-iterations", str(args.max_iterations),
    ]
    if args.max_budget_usd:
        cmd += ["--max-budget-usd", str(args.max_budget_usd)]
    if args.skip_permissions:
        cmd.append("--dangerously-skip-permissions")
    title = str(entry.get("title", "")) or format_todo(entry).splitlines()[0]
    print(
        f"\n[port-loop] pool: spawning todo worker for {title!r} "
        f"(attempt {attempts + 1}/5, base {base_sha[:12]}, worktree "
        f"{worktree}).",
        flush=True,
    )
    return subprocess.Popen(cmd, env=env, start_new_session=True)


def drive_port_pool(state: IterCounter, args: argparse.Namespace) -> None:
    """Supervisor: run up to `args.max_workers` porting tasks in parallel.

    Returns when the unported pool is drained, an admission gate flips,
    or all workers have been drained due to SIGINT. On return, control
    goes back to the outer loop for its next `repair → sync → PR CI →
    TODO` cycle, which re-evaluates admission before re-entering the
    pool.
    """
    supervisor_branch = _current_branch()
    if supervisor_branch in ("main", "master", "HEAD"):
        print(
            f"\n[port-loop] pool: refusing to run on branch "
            f"{supervisor_branch!r}; parallel porting only runs on "
            f"feature branches."
        )
        return

    N = args.max_workers
    # slot -> {proc, file, dest, base_sha, started_at}
    in_flight: dict[int, dict] = {}
    assigned: set[Path] = set()
    # The single TODO worker runs in parallel with the port workers and
    # uses its own dedicated worktree (`port/worker-todo`). At most one
    # TODO worker is in flight at any time. Wrapped in a list so the
    # signal-handler closures below can mutate it.
    todo_in_flight: list[dict | None] = [None]
    stop = [False]
    # SIGUSR1 sets drain=True: no new workers admitted, but in-flight workers
    # are left running and the pool waits for them to finish naturally.
    drain = [False]
    # Stagger spawns: allow first spawn immediately, then enforce the gap.
    last_spawn_at = time.monotonic() - _SPAWN_STAGGER_SECS
    last_todo_spawn_at = time.monotonic() - _SPAWN_STAGGER_SECS
    # Start with a fresh admission check so the outer loop's stale cache
    # (if any) doesn't let us enter under false-negative admission.
    _pool_admission_invalidate()
    # Clean up any worker branches that leaked to the remote in an
    # earlier run before we start minting new ones.
    _cleanup_remote_worker_branches()

    def _signal_worker_group(proc: subprocess.Popen, sig: int) -> None:
        """Signal the worker's whole process group.

        Workers are spawned with `start_new_session=True` so each one
        has its own PGID equal to its PID. Signaling the group cascades
        to the worker's own `claude` subprocesses (which live in the
        same group — `dispatch_claude` does NOT start a new session for
        them). Without this, SIGINT lands only on the worker's Python
        process and leaves claude running underneath.
        """
        try:
            os.killpg(proc.pid, sig)
        except (ProcessLookupError, PermissionError):
            pass

    def _live_count() -> int:
        return len(in_flight) + (1 if todo_in_flight[0] is not None else 0)

    def _handle_sigint(_signum, _frame):
        if not stop[0]:
            print(
                "\n[port-loop] pool: SIGINT received; propagating to "
                f"{_live_count()} worker(s) and draining.",
                flush=True,
            )
        stop[0] = True
        for info in in_flight.values():
            _signal_worker_group(info["proc"], signal.SIGINT)
        if todo_in_flight[0] is not None:
            _signal_worker_group(todo_in_flight[0]["proc"], signal.SIGINT)

    def _handle_sigusr1(_signum, _frame):
        if not drain[0]:
            print(
                "\n[port-loop] pool: SIGUSR1 received; draining gracefully "
                f"({_live_count()} worker(s) will finish, no new spawns).",
                flush=True,
            )
        drain[0] = True

    prev_handler = signal.signal(signal.SIGINT, _handle_sigint)
    prev_usr1_handler = signal.signal(signal.SIGUSR1, _handle_sigusr1)

    try:
        while True:
            # Before doing anything else, make sure we have headroom on
            # disk. Escalation (in `_emergency_disk_cleanup`): idle
            # worker dirs → supervisor `target/` → refuse admission.
            # Returns False only when even the escalation can't get us
            # above the BLOCKING threshold.
            disk_ok = _emergency_disk_cleanup(set(in_flight.keys()), N)

            # Try to spawn the TODO worker if it's idle and there's a
            # pickable entry in TODO.yaml. The TODO worker runs in
            # parallel with port workers and is independent of port
            # admission — it only stops on stop/drain/disk and on the
            # entry queue running dry. Skipped silently when the picker
            # returns None (queue empty, or every entry has exhausted
            # its retry budget).
            if (
                not stop[0]
                and not drain[0]
                and disk_ok
                and todo_in_flight[0] is None
                and time.monotonic() - last_todo_spawn_at >= _SPAWN_STAGGER_SECS
            ):
                picked = _pick_todo_for_dispatch()
                if picked is not None:
                    base_sha = git_head()
                    try:
                        proc = _spawn_todo_worker(
                            picked, base_sha, supervisor_branch, args,
                        )
                    except Exception as e:
                        print(
                            f"\n[port-loop] pool: failed to spawn todo "
                            f"worker: {e}"
                        )
                    else:
                        todo_in_flight[0] = {
                            "proc": proc,
                            "picked": picked,
                            "base_sha": base_sha,
                            "started_at": time.monotonic(),
                        }
                        last_todo_spawn_at = time.monotonic()

            # Admit into every free port-worker slot we can. Admission
            # is re-checked here so a CI-failure that appears mid-pool
            # stops new spawns without interrupting in-flight ones.
            if not stop[0] and not drain[0] and disk_ok:
                adm_ok, adm_reason = _pool_admission_ok()
                if (
                    not adm_ok
                    and not in_flight
                    and todo_in_flight[0] is None
                ):
                    print(
                        f"\n[port-loop] pool: admission denied "
                        f"({adm_reason}) and no workers in flight; "
                        f"returning to outer loop."
                    )
                    return
                if adm_ok and len(in_flight) < N:
                    pool = [
                        f for f in unported_pool()
                        if f not in assigned
                    ]
                    if pool:
                        # Rebase onto origin/main before spawning new
                        # tasks so workers start from fresh state. Done
                        # once per admission burst, not per slot. Workers
                        # already in flight will pick up the new head via
                        # their own post_rebase step when they finish.
                        sync_dispatched, _sync_pushed = sync_with_origin(
                            state
                        )
                        if sync_dispatched:
                            print(
                                "\n[port-loop] pool: sync_with_origin "
                                "dispatched a rescue agent; returning to "
                                "outer loop."
                            )
                            return
                    while len(in_flight) < N:
                        now = time.monotonic()
                        if now - last_spawn_at < _SPAWN_STAGGER_SECS:
                            break  # stagger: wait before spawning next worker
                        pool = [
                            f for f in unported_pool()
                            if f not in assigned
                        ]
                        if not pool:
                            break
                        file = random.choice(pool)
                        dest = destination_for(file)
                        base_sha = git_head()
                        slot = next(
                            i for i in range(N) if i not in in_flight
                        )
                        try:
                            proc = _spawn_worker(
                                slot, file, base_sha,
                                supervisor_branch, args,
                            )
                        except Exception as e:
                            print(
                                f"\n[port-loop] pool: failed to spawn "
                                f"worker {slot}: {e}"
                            )
                            stop[0] = True
                            break
                        in_flight[slot] = {
                            "proc": proc,
                            "file": file,
                            "dest": dest,
                            "base_sha": base_sha,
                            "started_at": time.monotonic(),
                        }
                        assigned.add(file)
                        last_spawn_at = time.monotonic()

            if not in_flight and todo_in_flight[0] is None:
                if stop[0] or drain[0]:
                    label = "stop requested" if stop[0] else "graceful drain complete"
                    print(
                        f"\n[port-loop] pool: {label} and all "
                        "workers drained; returning."
                    )
                    return
                if not disk_ok:
                    print(
                        "\n[port-loop] pool: disk is blocking admission "
                        "and no workers in flight; returning to outer "
                        "loop."
                    )
                    return
                if not unported_pool() and not read_todos():
                    print(
                        "\n[port-loop] pool: unported pool and TODO "
                        "queue are both empty; returning to outer loop."
                    )
                    return
                # Otherwise the only reason we admitted nothing is
                # admission denial or staggered re-spawn — handled at
                # top of next iteration.
                time.sleep(1.0)
                continue

            # Poll for any completed port worker plus the TODO worker.
            completed: list[int] = []
            for slot, info in in_flight.items():
                if info["proc"].poll() is not None:
                    completed.append(slot)
            todo_done = (
                todo_in_flight[0] is not None
                and todo_in_flight[0]["proc"].poll() is not None
            )
            if not completed and not todo_done:
                time.sleep(1.0)
                continue

            for slot in completed:
                info = in_flight.pop(slot)
                assigned.discard(info["file"])
                rc = info["proc"].returncode
                elapsed = time.monotonic() - info["started_at"]
                print(
                    f"\n[port-loop] pool: worker {slot} "
                    f"({info['file'].name}) exited rc={rc} after "
                    f"{elapsed:.1f}s."
                )
                _handle_worker_exit(
                    slot, info, rc, supervisor_branch, args, state,
                )

            if todo_done:
                info = todo_in_flight[0]
                todo_in_flight[0] = None
                rc = info["proc"].returncode
                elapsed = time.monotonic() - info["started_at"]
                print(
                    f"\n[port-loop] pool: todo worker exited rc={rc} "
                    f"after {elapsed:.1f}s."
                )
                _handle_todo_worker_exit(info, rc, supervisor_branch, state)

            # A worker completion may have added a TODO entry or pushed
            # commits that affect PR CI; force a fresh admission check.
            _pool_admission_invalidate()
    finally:
        # Escalate: SIGTERM → wait 30s → SIGKILL, all signaling the
        # worker's process group so `claude` under the worker also dies.
        live_procs = [info["proc"] for info in in_flight.values()]
        if todo_in_flight[0] is not None:
            live_procs.append(todo_in_flight[0]["proc"])
        for proc in live_procs:
            _signal_worker_group(proc, signal.SIGTERM)
        for proc in live_procs:
            try:
                proc.wait(timeout=30)
            except Exception:
                _signal_worker_group(proc, signal.SIGKILL)
                try:
                    proc.wait(timeout=5)
                except Exception:
                    pass
        signal.signal(signal.SIGINT, prev_handler)
        signal.signal(signal.SIGUSR1, prev_usr1_handler)


def _handle_worker_exit(
    slot: int,
    info: dict,
    rc: int,
    supervisor_branch: str,
    args: argparse.Namespace,
    state: IterCounter,
) -> None:
    """Integrate or rescue a completed worker's output."""
    file = info["file"]
    dest = info["dest"]
    base_sha = info["base_sha"]
    if rc == 0:
        ok, detail = _integrate_worker(slot, supervisor_branch, base_sha)
        if ok:
            print(f"[port-loop] pool: integrated worker {slot}: {detail}")
            return
        print(
            f"[port-loop] pool: worker {slot} integration failed: "
            f"{detail}"
        )
        _dispatch_merge_rescue(slot, file, detail, state)
        return
    if rc == 42:
        print(
            f"[port-loop] pool: worker {slot} exhausted per-file budget; "
            f"dispatching rescue in its worktree."
        )
        _dispatch_worker_rescue(
            slot, file, dest, args.per_file_dispatches, state,
        )
        ok, detail = _integrate_worker(slot, supervisor_branch, base_sha)
        if ok:
            print(
                f"[port-loop] pool: integrated rescue from worker "
                f"{slot}: {detail}"
            )
        else:
            print(
                f"[port-loop] pool: rescue integration failed: {detail}"
            )
            _dispatch_merge_rescue(slot, file, detail, state)
        return
    if rc == 43:
        print(
            f"[port-loop] pool: worker {slot} post-rebase unresolvable; "
            f"dispatching merge-rescue."
        )
        _dispatch_merge_rescue(
            slot, file,
            f"worker-side post-rebase failed for {file.name}", state,
        )
        return
    if rc < 0:
        print(
            f"[port-loop] pool: worker {slot} killed by signal "
            f"{-rc}; no integration attempted."
        )
        return
    print(
        f"[port-loop] pool: worker {slot} exited {rc} (unknown); "
        f"skipping integration."
    )


def _handle_todo_worker_exit(
    info: dict,
    rc: int,
    supervisor_branch: str,
    state: IterCounter,
) -> None:
    """Integrate the TODO worker's output.

    The TODO worker is single-shot: it dispatches one claude agent on a
    pre-picked entry and exits. On rc=0 we cherry-pick whatever commits
    landed on `port/worker-todo`. The supervisor's attempt counter for
    this entry was already bumped when the entry was picked, so any
    failure mode here just leaves the entry in the queue for next time.
    Cherry-pick conflicts are reported but not escalated to a rescue
    agent — the next pool tick will simply pick the next pickable
    entry, and the loop's outer repair will surface persistent issues.
    """
    base_sha = info["base_sha"]
    entry = info["picked"][0]
    title = str(entry.get("title", "")) or format_todo(entry).splitlines()[0]
    if rc == 0:
        ok, detail = _integrate_todo_worker(supervisor_branch, base_sha)
        if ok:
            print(
                f"[port-loop] pool: integrated todo worker ({title!r}): "
                f"{detail}"
            )
        else:
            print(
                f"[port-loop] pool: todo worker integration failed for "
                f"{title!r}: {detail}"
            )
        return
    if rc < 0:
        print(
            f"[port-loop] pool: todo worker killed by signal "
            f"{-rc}; no integration attempted."
        )
        return
    print(
        f"[port-loop] pool: todo worker exited {rc} ({title!r}); "
        f"skipping integration."
    )


def repair(state: IterCounter, run_server_tests: bool = False) -> None:
    any_failures = True
    while any_failures:
        any_failures = False
        apply_auto_fixes()
        ok, out, _ = gate_lint()
        if not ok:
            any_failures = True
            state.dispatch(LINT_FIX_PROMPT, gate_output=strip_build_noise(out))

        if run_server_tests:
            ok, out, perf = gate_server_tests()
            if not ok:
                state.dispatch(
                    test_fix_prompt_for(perf, SERVER_TEST_FIX_PROMPT),
                    gate_output=strip_build_noise(out),
                )
                any_failures = True

        ok, out, perf = gate_native_tests()
        if not ok:
            state.dispatch(
                test_fix_prompt_for(perf, NATIVE_TEST_FIX_PROMPT),
                gate_output=strip_build_noise(out),
            )
            any_failures = True
        if any_failures:
            continue
        ok, out = gate_clean_tree()
        if not ok:
            state.resume_last(COMMIT_PROMPT, gate_output=out)
            any_failures = True


def drive_todos(state: IterCounter) -> bool:
    """Dispatch one TODO entry if any are pending. Returns True iff dispatched.

    When dispatching cleared the last entry in TODO.yaml, runs a full
    `repair()` before returning so the outer loop sees a fresh green
    baseline before switching to porting new tests.

    Entry rotation: picks the entry with the fewest recorded attempts,
    with ties broken by original list order. This keeps a single stuck
    entry at the head of TODO.yaml from blocking all the others from
    ever being tried. A new entry (attempts=0) therefore gets first
    shot on the iteration it lands.

    Per-entry retry budget (persisted across restarts via
    `.port-loop-todo-attempts.json`, keyed by content hash so rewriting
    an entry resets its budget):
    - Attempts 1-4: dispatch `TODO_PROMPT` (normal).
    - Attempt 5: escalate to `TODO_RECOVERY_PROMPT` (rewrite/remove/skip).
    - Attempt 6+: the recovery agent didn't modify the entry either;
      skip this entry and fall through to porting.
    """
    todos = read_todos()
    if not todos:
        return False

    picked = _pick_todo_for_dispatch()
    if picked is None:
        print(
            f"\n[port-loop] all {len(todos)} TODO entry(ies) have exhausted "
            f"their attempt budget; blocking porting until a human intervenes."
        )
        return True

    entry, attempts, remaining, idx, entry_hash = picked
    title = str(entry.get("title", "")) or format_todo(entry).splitlines()[0]

    if attempts >= 4:
        print(
            f"\n[port-loop] TODO {title!r} (hash {entry_hash}) still present "
            f"after {attempts} attempts; dispatching recovery agent."
        )
        state.dispatch(
            TODO_RECOVERY_PROMPT.format(
                entry=format_todo(entry), attempts=attempts
            )
        )
    else:
        print(
            f"\n[port-loop] {len(todos)} TODO entry(ies) pending; picked "
            f"{title!r} at index {idx} (attempt {attempts + 1}/5, lowest in "
            f"the queue)."
        )
        state.dispatch(
            TODO_PROMPT.format(entry=format_todo(entry), remaining=remaining)
        )

    if remaining == 0 and not read_todos():
        print(
            f"\n[port-loop] last TODO cleared; running a full repair before "
            f"moving on to porting."
        )
        repair(state)
    return True


def _pick_todo_for_dispatch() -> (
    tuple[dict, int, int, int, str] | None
):
    """Pick the next TODO entry to dispatch, bumping its attempt counter.

    Returns `(entry, attempts_before_bump, remaining, idx, entry_hash)` —
    the same fields `drive_todos` and the parallel TODO worker need.
    Returns `None` if TODO.yaml is empty or every entry has exhausted
    its attempt budget (5 normal + 1 recovery).

    Bumps the picked entry's counter and persists it immediately so
    concurrent calls (e.g. supervisor re-pick after a worker crash)
    don't re-pick the same entry on the same attempt.
    """
    todos = read_todos()
    if not todos:
        return None

    attempts_map = _load_todo_attempts()
    hashes = [_todo_hash(t) for t in todos]
    attempts_map = _prune_todo_attempts(attempts_map, set(hashes))

    # Rank (attempts_so_far, original_index) ascending.
    ranked = sorted(
        range(len(todos)),
        key=lambda i: (attempts_map.get(hashes[i], 0), i),
    )
    pickable = [i for i in ranked if attempts_map.get(hashes[i], 0) < 6]
    if not pickable:
        return None

    idx = pickable[0]
    entry = todos[idx]
    entry_hash = hashes[idx]
    attempts = attempts_map.get(entry_hash, 0)
    remaining = len(todos) - 1

    attempts_map[entry_hash] = attempts + 1
    _save_todo_attempts(attempts_map)

    return entry, attempts, remaining, idx, entry_hash


def _run_git(args: list[str]) -> subprocess.CompletedProcess:
    """Run a git subcommand, capturing output and echoing it live."""
    cmd = ["git", *args]
    print(f"\n$ {' '.join(cmd)}")
    proc = subprocess.run(
        cmd,
        cwd=REPO_ROOT,
        capture_output=True,
        text=True,
        check=False,
    )
    if proc.stdout:
        sys.stdout.write(proc.stdout)
    if proc.stderr:
        sys.stdout.write(proc.stderr)
    sys.stdout.flush()
    return proc


def _current_branch() -> str:
    r = subprocess.run(
        ["git", "rev-parse", "--abbrev-ref", "HEAD"],
        cwd=REPO_ROOT,
        capture_output=True,
        text=True,
        check=True,
    )
    return r.stdout.strip()


def _rev_parse(rev: str) -> str | None:
    r = subprocess.run(
        ["git", "rev-parse", "--verify", "--quiet", rev],
        cwd=REPO_ROOT,
        capture_output=True,
        text=True,
        check=False,
    )
    if r.returncode != 0:
        return None
    return r.stdout.strip()


PUSH_MIN_INTERVAL_SECONDS = 60 * 60  # at most one push per hour
LAST_PUSH_PATH = REPO_ROOT / ".port-loop-last-push"


def _seconds_since_last_push() -> float | None:
    """Seconds since the most recent successful push recorded by this script.

    Returns None if no push has been recorded (e.g. first run, or the
    timestamp file was cleared).
    """
    try:
        ts = float(LAST_PUSH_PATH.read_text().strip())
    except (FileNotFoundError, ValueError, OSError):
        return None
    return time.time() - ts


def _record_push_now() -> None:
    LAST_PUSH_PATH.write_text(f"{time.time():.0f}\n")


def gate_sync_with_origin() -> tuple[bool, str, bool]:
    """Fetch, rebase onto origin/main, push. Returns (ok, output, pushed).

    Called only after local gates are green and the tree is clean. Does
    nothing destructive: always uses `--force-with-lease`, never
    `--force`. Refuses to operate on the `main`/`master` branch itself
    (the loop should run on a feature branch).

    Pushes are rate-limited to at most one every
    `PUSH_MIN_INTERVAL_SECONDS` *while CI is green*: if a push would
    happen sooner than that, the fetch/rebase still runs but the push
    is deferred to the next sync cycle that falls outside the
    cooldown. This keeps CI from churning on every committed port
    when workers are landing commits rapidly.

    The throttle is bypassed when CI is currently broken (status
    `"failure"`, or any already-completed check is failing) — push
    fixes immediately. First-publish pushes (no existing upstream
    branch) also bypass the throttle.

    On any failure, aborts any in-progress rebase and returns (False,
    combined_output, False) so the caller can dispatch a recovery agent.
    """
    branch = _current_branch()
    if branch in ("main", "master", "HEAD"):
        print(
            f"\n[port-loop] on branch {branch!r}; skipping sync-with-origin. "
            f"The sync gate only runs on feature branches."
        )
        return True, "", False

    combined: list[str] = []

    def _record(proc: subprocess.CompletedProcess) -> None:
        combined.append(f"$ {' '.join(proc.args)}\n")
        if proc.stdout:
            combined.append(proc.stdout)
        if proc.stderr:
            combined.append(proc.stderr)

    fetch = _run_git(["fetch", "origin"])
    _record(fetch)
    if fetch.returncode != 0:
        return False, "".join(combined), False

    before_sha = git_head()
    origin_main = _rev_parse("origin/main")
    if origin_main is None:
        # No origin/main — likely a fresh clone or misconfigured remote.
        # Leave the rebase step out and just try to push what we have.
        print(
            "\n[port-loop] no origin/main to rebase onto; skipping rebase."
        )
    else:
        rebase = _run_git(["rebase", "origin/main"])
        _record(rebase)
        if rebase.returncode != 0:
            abort = _run_git(["rebase", "--abort"])
            _record(abort)
            return False, "".join(combined), False

    # Decide whether we need to push. If origin already has our exact
    # HEAD, this is a no-op.
    after_sha = git_head()
    origin_branch = _rev_parse(f"origin/{branch}")
    needs_push = origin_branch is None or origin_branch != after_sha

    if not needs_push:
        return True, "".join(combined), False

    # Rate-limit: skip if we pushed recently AND CI isn't broken.
    # New-upstream pushes (origin_branch is None) bypass the throttle
    # so a fresh branch gets its initial publish without delay.
    # Broken CI also bypasses — fixes should go out immediately.
    if origin_branch is not None:
        elapsed = _seconds_since_last_push()
        if elapsed is not None and elapsed < PUSH_MIN_INTERVAL_SECONDS:
            ci_status, ci_summary, _d, ci_failing = _pr_check_status(
                fetch_logs=False
            )
            ci_broken = ci_status == "failure" or ci_failing > 0
            if not ci_broken:
                remaining = PUSH_MIN_INTERVAL_SECONDS - elapsed
                msg = (
                    f"[port-loop] push throttle: last push "
                    f"{int(elapsed)}s ago, next push in {int(remaining)}s "
                    f"(min interval {PUSH_MIN_INTERVAL_SECONDS}s, "
                    f"CI={ci_status}/{ci_summary}). "
                    f"Deferring push of {after_sha[:12]}.\n"
                )
                print(msg, end="")
                combined.append(msg)
                return True, "".join(combined), False
            msg = (
                f"[port-loop] push throttle bypassed: CI broken "
                f"({ci_status}, {ci_failing} failing), "
                f"pushing {after_sha[:12]} immediately.\n"
            )
            print(msg, end="")
            combined.append(msg)

    # Push, retrying up to 3 times on --force-with-lease rejection.
    # Workers may push to the remote while we are rebasing, causing a
    # lease mismatch; a re-fetch + re-rebase usually resolves it without
    # needing to dispatch an agent.
    _PUSH_RETRIES = 3
    for _attempt in range(_PUSH_RETRIES):
        if origin_branch is None:
            push = _run_git(["push", "--set-upstream", "origin", "HEAD"])
        else:
            push = _run_git(["push", "--force-with-lease", "origin", "HEAD"])
        _record(push)
        if push.returncode == 0:
            break
        if _attempt < _PUSH_RETRIES - 1:
            print(
                f"\n[port-loop] push attempt {_attempt + 1} failed "
                f"(force-with-lease mismatch?); re-fetching and retrying.",
                flush=True,
            )
            retry_fetch = _run_git(["fetch", "origin"])
            _record(retry_fetch)
            if retry_fetch.returncode != 0:
                return False, "".join(combined), False
            if origin_main is not None:
                retry_rebase = _run_git(["rebase", "origin/main"])
                _record(retry_rebase)
                if retry_rebase.returncode != 0:
                    abort = _run_git(["rebase", "--abort"])
                    _record(abort)
                    return False, "".join(combined), False
            origin_branch = _rev_parse(f"origin/{branch}")
    else:
        return False, "".join(combined), False
    _record_push_now()

    rebased = origin_main is not None and before_sha != after_sha
    if rebased:
        print(
            f"\n[port-loop] rebased {before_sha[:12]} → {after_sha[:12]} "
            f"and pushed."
        )
    else:
        print(f"\n[port-loop] pushed {after_sha[:12]} to origin/{branch}.")
    return True, "".join(combined), True


def sync_with_origin(state: IterCounter) -> tuple[bool, bool]:
    """Run the sync gate. Returns (dispatched, pushed).

    `dispatched` is True if the gate failed and an agent was dispatched
    — the caller should `continue` the outer loop in that case.
    `pushed` is True if the sync resulted in an actual push; the caller
    uses this to skip the PR CI check for one iteration so GitHub has
    time to kick off the new run.
    """
    ok, out, pushed = gate_sync_with_origin()
    if not ok:
        state.dispatch(SYNC_FIX_PROMPT, gate_output=out)
        return True, False
    return False, pushed


# ---- parallel-worker worktree pool ------------------------------------------
#
# When `--max-workers > 1`, the porting phase is driven out of a fixed pool
# of git worktrees under `.port-worktrees/worker-{i}/`. Each worktree is
# pinned to its own branch `port/worker-{i}` and its own Cargo target
# directory `target-worker-{i}/` (absolute path, so every worker using slot
# i reuses the same incremental-build state). The main worktree (where the
# supervisor runs) is unchanged; workers don't touch it directly. Workers
# commit locally in their own worktrees; the supervisor cherry-picks their
# commits onto the supervisor branch serially as each worker finishes.

WORKTREE_ROOT = REPO_ROOT / ".port-worktrees"


def _worker_path(i: int) -> Path:
    return WORKTREE_ROOT / f"worker-{i}"


def _worker_branch(i: int) -> str:
    return f"port/worker-{i}"


def _worker_target_dir(i: int) -> Path:
    return REPO_ROOT / f"target-worker-{i}"


# Per-worker CARGO_TARGET_DIR size above which we nuke-and-rebuild before
# spawning the next task in that slot. Test-binary artifacts accumulate
# as ports land fast. Four workers at 4 GiB each + supervisor `target/`
# fits comfortably on a 123 GiB disk; rebuilding from empty costs one
# slow cycle per trim but keeps disk usage bounded.
WORKER_TARGET_SIZE_LIMIT_BYTES = 4 * 1024 * 1024 * 1024  # 4 GiB

# Staircase of free-disk thresholds. Below the `EMERGENCY` threshold we
# wipe idle worker target dirs. If that still leaves us below the
# `CRITICAL` threshold, we escalate by wiping the supervisor `target/`
# too (rebuild cost: the next outer-loop gate). Below `BLOCKING` we
# refuse admission of new workers and drain — pushing a build into
# the last couple of GiB is the recipe for ENOSPC mid-link.
EMERGENCY_DISK_FREE_BYTES = 20 * 1024 * 1024 * 1024  # 20 GiB
CRITICAL_DISK_FREE_BYTES = 10 * 1024 * 1024 * 1024   # 10 GiB
BLOCKING_DISK_FREE_BYTES = 5 * 1024 * 1024 * 1024    # 5 GiB


def _dir_size_bytes(path: Path) -> int:
    """Return `path`'s recursive size in bytes, or 0 if it doesn't exist.

    Uses `du -sb` for speed — a pure-Python walk of a 15 GB target dir
    takes noticeable wall time while `du` completes in well under a
    second on these volumes.
    """
    if not path.exists():
        return 0
    r = _run_capture(["du", "-sb", str(path)])
    if r.returncode != 0 or not r.stdout:
        return 0
    try:
        return int(r.stdout.split(maxsplit=1)[0])
    except (ValueError, IndexError):
        return 0


def _disk_free_bytes() -> int:
    """Free bytes on the filesystem holding REPO_ROOT."""
    try:
        return shutil.disk_usage(str(REPO_ROOT)).free
    except OSError:
        return 0


def _maybe_clean_worker_target(slot: int) -> None:
    """Delete worker `slot`'s CARGO_TARGET_DIR if it's over the size limit."""
    target = _worker_target_dir(slot)
    size = _dir_size_bytes(target)
    if size <= WORKER_TARGET_SIZE_LIMIT_BYTES:
        return
    gib = size / (1024 ** 3)
    limit_gib = WORKER_TARGET_SIZE_LIMIT_BYTES / (1024 ** 3)
    print(
        f"\n[port-loop] pool: worker {slot} target-dir is "
        f"{gib:.1f} GiB (> {limit_gib:.0f} GiB); rm -rf to reclaim.",
        flush=True,
    )
    subprocess.run(["rm", "-rf", str(target)], check=False)


def _emergency_disk_cleanup(busy_slots: set[int], max_slots: int) -> bool:
    """Escalating disk-space reclaim. Returns True if disk is now OK.

    `busy_slots` = slot indices with a live worker writing to their
    target dir; those are left alone. `max_slots` = configured pool
    size. Escalation:

    1. Free >= EMERGENCY: no-op, return True.
    2. Free < EMERGENCY: wipe all idle worker target dirs.
    3. Still < CRITICAL: also wipe the supervisor `target/`. The next
       outer-loop gate will rebuild — that's one slow cycle, not per
       port. Safe inside `drive_port_pool` because cargo invocations
       from the supervisor only happen *between* pool cycles (in the
       outer repair/sync gates), never while we're here.
    4. Still < BLOCKING: return False so the caller refuses admission
       and drains in-flight workers.

    Cheap when disk is healthy (one `statvfs` call); only fans out
    into `rm -rf` when actually needed.
    """
    free = _disk_free_bytes()
    if free >= EMERGENCY_DISK_FREE_BYTES:
        return True

    def _free_gib() -> float:
        return _disk_free_bytes() / (1024 ** 3)

    print(
        f"\n[port-loop] pool: emergency — only {_free_gib():.1f} GiB free "
        f"(< {EMERGENCY_DISK_FREE_BYTES / (1024 ** 3):.0f} GiB); "
        f"wiping idle worker target dirs.",
        flush=True,
    )
    for slot in range(max_slots):
        if slot in busy_slots:
            continue
        target = _worker_target_dir(slot)
        if target.exists():
            size_gib = _dir_size_bytes(target) / (1024 ** 3)
            print(
                f"[port-loop] pool: rm -rf {target.name} ({size_gib:.1f} GiB)",
                flush=True,
            )
            subprocess.run(["rm", "-rf", str(target)], check=False)

    if _disk_free_bytes() < CRITICAL_DISK_FREE_BYTES:
        supervisor_target = REPO_ROOT / "target"
        if supervisor_target.exists():
            size_gib = _dir_size_bytes(supervisor_target) / (1024 ** 3)
            print(
                f"[port-loop] pool: still {_free_gib():.1f} GiB free after "
                f"worker cleanup; wiping supervisor target/ ({size_gib:.1f} "
                f"GiB). Next outer-loop gate will rebuild.",
                flush=True,
            )
            subprocess.run(
                ["rm", "-rf", str(supervisor_target)], check=False,
            )

    free_after = _disk_free_bytes()
    free_after_gib = free_after / (1024 ** 3)
    if free_after < BLOCKING_DISK_FREE_BYTES:
        print(
            f"\n[port-loop] pool: CRITICAL — still only {free_after_gib:.1f} "
            f"GiB free after all cleanup. Refusing admission; draining "
            f"in-flight workers.",
            flush=True,
        )
        return False
    print(
        f"[port-loop] pool: reclaimed to {free_after_gib:.1f} GiB free.",
        flush=True,
    )
    return True


def _run_capture(args: list[str], *, cwd: Path | None = None) -> subprocess.CompletedProcess:
    """Quiet git/subprocess runner that captures stdout+stderr together."""
    return subprocess.run(
        args,
        cwd=str(cwd) if cwd else None,
        capture_output=True,
        text=True,
        check=False,
    )


def _ensure_worktree_at(
    path: Path, branch: str, base_sha: str, label: str
) -> Path:
    """Create the worktree at `path` pinned to `base_sha` (reuses if present)."""
    WORKTREE_ROOT.mkdir(parents=True, exist_ok=True)

    if path.exists() and (path / ".git").exists():
        _reset_worktree_at(path, branch, base_sha)
        return path

    if path.exists():
        # Stale filesystem leftover — rm -rf via subprocess (no Python
        # recursion since the tree may have gitignored subdirs).
        subprocess.run(["rm", "-rf", str(path)], check=True)

    # Prune first in case a previous run left a dangling worktree registration.
    _run_capture(["git", "worktree", "prune"], cwd=REPO_ROOT)

    add = _run_capture(
        ["git", "worktree", "add", "-B", branch, str(path), base_sha],
        cwd=REPO_ROOT,
    )
    if add.returncode != 0:
        # Branch may already be checked out in a stale registration — force.
        _run_capture(
            ["git", "worktree", "remove", "--force", str(path)],
            cwd=REPO_ROOT,
        )
        _run_capture(["git", "branch", "-D", branch], cwd=REPO_ROOT)
        add = _run_capture(
            ["git", "worktree", "add", "-B", branch, str(path), base_sha],
            cwd=REPO_ROOT,
        )
        if add.returncode != 0:
            raise RuntimeError(
                f"failed to create worktree for {label} at {path}: "
                f"{add.stdout}\n{add.stderr}"
            )
    return path


def _reset_worktree_at(path: Path, branch: str, base_sha: str) -> None:
    """Reset `path`'s branch to `base_sha` and clean untracked files.

    Keeps `.port-loop-cache.json`, the porting sessions dir, and the
    Cargo target (which lives outside the worktree anyway) so successive
    runs don't start from cold every time.
    """
    ck = _run_capture(
        ["git", "checkout", "-B", branch, base_sha],
        cwd=path,
    )
    if ck.returncode != 0:
        raise RuntimeError(
            f"checkout -B {branch} {base_sha} failed in {path}: "
            f"{ck.stdout}\n{ck.stderr}"
        )
    rs = _run_capture(["git", "reset", "--hard", base_sha], cwd=path)
    if rs.returncode != 0:
        raise RuntimeError(
            f"reset --hard {base_sha} failed in {path}: "
            f"{rs.stdout}\n{rs.stderr}"
        )
    _run_capture(
        [
            "git", "clean", "-fdx",
            "-e", ".port-loop-cache.json",
            "-e", ".port-loop-todo-attempts.json",
            "-e", ".porting/",
        ],
        cwd=path,
    )


def _ensure_worktree(i: int, base_sha: str) -> Path:
    """Create worker `i`'s worktree pinned to `base_sha` (reuses if present)."""
    return _ensure_worktree_at(
        _worker_path(i), _worker_branch(i), base_sha, f"worker {i}"
    )


def _reset_worktree(i: int, base_sha: str) -> None:
    """Reset worker `i`'s branch to `base_sha` and clean untracked files."""
    _reset_worktree_at(_worker_path(i), _worker_branch(i), base_sha)


def _ensure_todo_worktree(base_sha: str) -> Path:
    """Create the TODO worker's worktree pinned to `base_sha`."""
    return _ensure_worktree_at(
        _todo_worker_path(), _todo_worker_branch(), base_sha, "todo worker"
    )


def _cleanup_remote_worker_branches() -> None:
    """Delete any `port/worker-*` branches that leaked to origin.

    Worker branches exist only as integration targets for the
    cherry-pick back to the supervisor branch; they must never live on
    the remote. If a sub-agent (or a stale run) pushed one, remove it
    and unset any branch-tracking config that would make future `git
    push` calls repeat the mistake.
    """
    r = _run_capture(
        ["git", "branch", "-r", "--list", "origin/port/worker-*"],
        cwd=REPO_ROOT,
    )
    if r.returncode != 0:
        return
    for line in r.stdout.splitlines():
        name = line.strip()
        if not name.startswith("origin/port/worker-"):
            continue
        branch = name[len("origin/"):]
        print(
            f"\n[port-loop] pool: deleting leaked origin/{branch}.",
            flush=True,
        )
        _run_capture(
            ["git", "push", "origin", "--delete", branch],
            cwd=REPO_ROOT,
        )
    # Clear branch.<port/worker-*>.{remote,merge,pushRemote} entries
    # in the main config so subsequent `git push` from a worker won't
    # auto-push to origin.
    cfg = _run_capture(
        ["git", "config", "--get-regexp", r"^branch\.port/worker-.*"],
        cwd=REPO_ROOT,
    )
    if cfg.returncode == 0:
        seen: set[str] = set()
        for line in cfg.stdout.splitlines():
            key = line.split(" ", 1)[0] if " " in line else line
            # key looks like branch.port/worker-0.remote
            parts = key.rsplit(".", 1)
            if len(parts) != 2:
                continue
            section = parts[0]  # branch.port/worker-0
            if section in seen:
                continue
            seen.add(section)
            _run_capture(
                ["git", "config", "--remove-section", section],
                cwd=REPO_ROOT,
            )


def _integrate_branch(
    branch: str, supervisor_branch: str, base_sha: str, label: str
) -> tuple[bool, str]:
    """Replay `branch`'s commits onto the supervisor branch.

    Uses `git cherry-pick` so the supervisor branch linearly accumulates
    every worker's commits without needing fast-forward. Returns
    `(ok, detail)` — on conflict the cherry-pick is aborted and the
    caller is expected to dispatch a merge-rescue agent.
    """
    worker_sha = _rev_parse(branch)
    if worker_sha is None:
        return False, f"{label} branch {branch} is missing"
    if worker_sha == base_sha:
        return True, f"{label} produced no commits; nothing to integrate"
    head_branch = _current_branch()
    if head_branch != supervisor_branch:
        return False, (
            f"supervisor-branch drift: expected {supervisor_branch}, "
            f"on {head_branch}"
        )
    range_spec = f"{base_sha}..{branch}"
    pick = _run_capture(
        [
            "git", "cherry-pick",
            "--allow-empty", "--keep-redundant-commits",
            range_spec,
        ],
        cwd=REPO_ROOT,
    )
    if pick.returncode == 0:
        return True, f"cherry-picked {range_spec}"
    _run_capture(["git", "cherry-pick", "--abort"], cwd=REPO_ROOT)
    return False, (
        f"cherry-pick {range_spec} failed:\n{pick.stdout}\n{pick.stderr}"
    )


def _integrate_worker(
    i: int, supervisor_branch: str, base_sha: str
) -> tuple[bool, str]:
    """Replay worker `i`'s commits onto the supervisor branch."""
    return _integrate_branch(
        _worker_branch(i), supervisor_branch, base_sha, f"worker {i}"
    )


def _integrate_todo_worker(
    supervisor_branch: str, base_sha: str
) -> tuple[bool, str]:
    """Replay the TODO worker's commits onto the supervisor branch."""
    return _integrate_branch(
        _todo_worker_branch(), supervisor_branch, base_sha, "todo worker"
    )


def _session_tail(session_id: str, max_lines: int = 200) -> str:
    """Return the last `max_lines` lines of a session's jsonl log.

    Used to build the rescue prompt — the agent needs to know what the
    worker tried before giving up.
    """
    log = SESSIONS_DIR / f"{session_id}.jsonl"
    if not log.exists():
        return "(session log not found)"
    try:
        content = log.read_text(encoding="utf-8", errors="replace").splitlines()
    except OSError as e:
        return f"(could not read session log: {e})"
    tail = content[-max_lines:]
    return "\n".join(tail)


# ---- PR CI gate -------------------------------------------------------------


_CI_SIGNAL_RE = re.compile(
    r"(FAILED|"
    r"panicked at|thread .* panicked|assertion (failed|`left)|"
    r"error(\[E\d+\])?:|warning: unused|"
    r"test result:|"
    r"^Uncovered|Coverage check (FAILED|PASSED)|"
    r"^\s*---- .+ stdout ----|^failures:|"
    r"note: run with `RUST_BACKTRACE|"
    r"Error: Process completed with exit code)",
    re.MULTILINE,
)


def _extract_failure_lines(raw: str, max_chars: int = 6000) -> str:
    """Keep lines matching CI-failure patterns, plus a little context.

    Logs from `gh run view --log-failed` can be tens of thousands of
    lines; the fix agent only needs the failure markers and their
    nearby context. Returns a trimmed string capped at `max_chars`.
    """
    if not raw.strip():
        return "(empty log)"
    lines = raw.splitlines()
    keep = [False] * len(lines)
    for i, line in enumerate(lines):
        if _CI_SIGNAL_RE.search(line):
            for j in range(max(0, i - 1), min(len(lines), i + 8)):
                keep[j] = True
    kept = [ln for ln, k in zip(lines, keep) if k]
    out = "\n".join(kept).strip()
    if not out:
        # No signal matched; fall back to the last ~80 lines so the
        # agent at least has something to work with.
        out = "\n".join(lines[-80:])
    if len(out) > max_chars:
        out = out[:max_chars].rstrip() + "\n... (truncated)\n"
    return out


def _fetch_run_log_summary(run_id: str) -> str:
    """Fetch `gh run view --log-failed` for a run id and extract signal."""
    try:
        proc = subprocess.run(
            [
                "gh", "run", "view", run_id,
                "--log-failed", "--repo", PR_REPO,
            ],
            capture_output=True, text=True, timeout=180, check=False,
        )
    except (subprocess.TimeoutExpired, FileNotFoundError) as e:
        return f"(gh run view failed: {e})"
    raw = proc.stdout + ("\n" + proc.stderr if proc.stderr else "")
    return _extract_failure_lines(raw)


def _check_run_bucket(c: dict) -> str:
    """Map a GitHub check-run object to a bucket: pending/pass/fail/cancel."""
    status = c.get("status", "")
    conclusion = c.get("conclusion") or ""
    if status != "completed":
        return "pending"
    if conclusion in ("success", "skipped", "neutral"):
        return "pass"
    if conclusion == "cancelled":
        return "cancel"
    return "fail"  # failure, timed_out, action_required


def _pr_check_status(fetch_logs: bool = True) -> tuple[str, str, str, int]:
    """Return (status, summary, detail, failing_count) for the tracked PR's CI.

    `status` is one of `"skip"` (can't tell / PR closed / no checks),
    `"pending"`, `"success"`, `"failure"`. `summary` is a short label
    for logs. `detail` is a pre-extracted failing-log summary for the
    triage agent (empty for statuses other than `"failure"`, or when
    `fetch_logs=False`). `failing_count` is the number of already-
    completed checks that failed or were cancelled — reported even
    when `status == "pending"` so callers can distinguish "pending but
    clean" from "pending with some checks already failing".

    Uses `gh api` directly (avoids `gh pr checks --json` which is
    absent in some packaged versions of gh). Set `fetch_logs=False`
    when the caller only needs status/summary/failing_count — avoids
    running `gh run view --log-failed` (slow + API-quota expensive).
    """
    # Step 1: resolve PR head SHA.
    try:
        sha_result = subprocess.run(
            ["gh", "api", f"repos/{PR_REPO}/pulls/{PR_NUMBER}",
             "--jq", ".head.sha"],
            capture_output=True, text=True, timeout=60, check=False,
        )
    except (subprocess.TimeoutExpired, FileNotFoundError) as e:
        return "skip", f"gh unavailable: {e}", "", 0
    if sha_result.returncode != 0:
        return "skip", f"gh api PR failed: {sha_result.stderr.strip()}", "", 0
    sha = sha_result.stdout.strip()
    if not sha:
        return "skip", "could not determine PR head SHA", "", 0

    # Step 2: fetch check runs for that commit.
    try:
        runs_result = subprocess.run(
            ["gh", "api", f"repos/{PR_REPO}/commits/{sha}/check-runs",
             "--paginate"],
            capture_output=True, text=True, timeout=60, check=False,
        )
    except (subprocess.TimeoutExpired, FileNotFoundError) as e:
        return "skip", f"gh unavailable: {e}", "", 0
    if runs_result.returncode != 0:
        return "skip", f"gh api check-runs failed: {runs_result.stderr.strip()}", "", 0
    try:
        data = json.loads(runs_result.stdout or "{}")
    except json.JSONDecodeError:
        return "skip", "gh api returned non-JSON", "", 0
    checks = data.get("check_runs", [])
    if not checks:
        return "skip", "no checks reported yet", "", 0

    buckets = [_check_run_bucket(c) for c in checks]
    failing_count = sum(1 for b in buckets if b in ("fail", "cancel"))
    if any(b == "pending" for b in buckets):
        n = sum(1 for b in buckets if b == "pending")
        summary = f"{n}/{len(checks)} checks still running"
        if failing_count:
            summary += f" ({failing_count} already failing)"
        return "pending", summary, "", failing_count
    failing = [c for c in checks if _check_run_bucket(c) in ("fail", "cancel")]
    if failing:
        names = [c.get("name", "?") for c in failing]
        if fetch_logs:
            # Dedup run ids: multiple failing jobs can share one workflow run.
            run_ids: list[str] = []
            for c in failing:
                url = c.get("html_url") or c.get("details_url") or ""
                m = re.search(r"/runs/(\d+)", url)
                if m and m.group(1) not in run_ids:
                    run_ids.append(m.group(1))
            sections: list[str] = []
            for rid in run_ids:
                print(
                    f"[port-loop] fetching --log-failed for run {rid}",
                    flush=True,
                )
                sections.append(
                    f"=== run {rid} ===\n{_fetch_run_log_summary(rid)}"
                )
            if not sections:
                sections.append("(no run-id parseable from check URLs)")
            detail = "\n\n".join(sections)
        else:
            detail = ""
        return (
            "failure",
            f"{len(failing)} failing: {', '.join(names)}",
            detail,
            len(failing),
        )
    return "success", f"all {len(checks)} checks passing", "", 0


def has_ci_todos() -> bool:
    """True if TODO.yaml has at least one `[CI]`-prefixed entry."""
    for entry in read_todos():
        if isinstance(entry, dict):
            title = entry.get("title")
            if isinstance(title, str) and title.startswith("[CI]"):
                return True
    return False


def gate_pr_ci() -> tuple[bool, str]:
    """Returns (ok, detail). ok=False only on completed-failure."""
    print(f"\n$ gh pr checks {PR_NUMBER} --repo {PR_REPO}")
    status, summary, detail, _ = _pr_check_status()
    print(f"[port-loop] PR #{PR_NUMBER}: {status} — {summary}", flush=True)
    return status != "failure", detail


def _wait_for_new_pr_checks(grace: float = 120.0) -> bool:
    """Poll until at least one PR check is in `pending` state.

    Called immediately after dispatching a CI fix agent that is expected
    to have committed + pushed. Returns True if pending checks appeared
    within the grace window (so we can usefully block on `--watch`);
    False if the window elapsed with only terminal states visible
    (meaning either the agent didn't push, or GitHub hasn't registered
    the push yet — in which case `drive_pr_ci` will re-dispatch on the
    next outer iteration rather than blocking on stale checks).
    """
    deadline = time.monotonic() + grace
    while time.monotonic() < deadline:
        status, summary, _, _ = _pr_check_status()
        print(
            f"[port-loop] PR CI wait-for-new-checks: {status} — {summary}",
            flush=True,
        )
        if status == "pending":
            return True
        time.sleep(10)
    return False


def _watch_pr_ci(timeout: float = 3600.0) -> None:
    """Stream `gh pr checks --watch` until all checks complete (or timeout)."""
    print(f"\n$ gh pr checks {PR_NUMBER} --repo {PR_REPO} --watch")
    try:
        subprocess.run(
            [
                "gh",
                "pr",
                "checks",
                str(PR_NUMBER),
                "--repo",
                PR_REPO,
                "--watch",
            ],
            check=False,
            timeout=timeout,
        )
    except subprocess.TimeoutExpired:
        print("[port-loop] gh pr checks --watch timed out")
    except FileNotFoundError as e:
        print(f"[port-loop] gh unavailable: {e}")


_ci_seen_green_since_failure = False


def drive_pr_ci(state: IterCounter, *, just_pushed: bool = False) -> bool:
    """Act on the upstream PR's CI state.

    - "success" / "skip": return False (no action).
    - "pending": if we've previously observed a fully-green CI and
      no failures have appeared since, skip the watch and return
      False so the loop keeps working while checks run in the
      background. Otherwise block on `gh pr checks --watch` and
      re-evaluate on the next pass.
    - "failure": if TODO.yaml already has `[CI]` entries, return True so
      the outer loop drains them via drive_todos. Otherwise dispatch
      the triage agent (which writes `[CI]` entries to TODO.yaml) and
      return True.

    Returns True if any action was taken; False only when CI was
    already green, skipped, or safe to ignore while pending.
    """
    global _ci_seen_green_since_failure

    if has_ci_todos():
        print(
            "[port-loop] TODO.yaml already has [CI] entries; "
            "skipping CI gate and letting drive_todos drain them."
        )
        # A [CI] TODO exists only because we saw a failure; the
        # green-baseline flag is stale until the triage cycle
        # completes and CI goes green again.
        _ci_seen_green_since_failure = False
        return False  # fall through to drive_todos in the outer loop

    if just_pushed:
        _wait_for_new_pr_checks()

    status, summary, detail, failing_count = _pr_check_status()
    print(f"[port-loop] PR #{PR_NUMBER}: {status} — {summary}", flush=True)
    if status == "success":
        _ci_seen_green_since_failure = True
        return False
    if status == "skip":
        return False
    if status == "pending":
        if failing_count == 0 and _ci_seen_green_since_failure:
            print(
                "[port-loop] PR CI pending with no failures yet and a "
                "green baseline already observed; continuing without "
                "blocking on --watch."
            )
            return False
        if failing_count:
            _ci_seen_green_since_failure = False
        _watch_pr_ci()
        return True
    # status == "failure"
    _ci_seen_green_since_failure = False
    state.dispatch(
        CI_TRIAGE_PROMPT.format(repo=PR_REPO, pr=PR_NUMBER),
        gate_output=detail,
    )
    return True


def resolve_port_arg(raw: str) -> Path:
    """Resolve a --port argument to an upstream Path; abort on invalid input."""
    candidate = Path(raw)
    if not candidate.is_absolute():
        candidate = (REPO_ROOT / candidate).resolve()
    else:
        candidate = candidate.resolve()
    if not candidate.is_file():
        sys.exit(f"[port-loop] --port: no such file: {raw}")
    try:
        candidate.relative_to(PBTKIT_DIR)
    except ValueError:
        try:
            candidate.relative_to(HYPOTHESIS_DIR)
        except ValueError:
            sys.exit(
                f"[port-loop] --port: {raw} is not under {PBTKIT_DIR} "
                f"or {HYPOTHESIS_DIR}"
            )
    if not candidate.name.startswith("test_") or candidate.suffix != ".py":
        sys.exit(
            f"[port-loop] --port: {raw} does not look like a test_*.py file"
        )
    return candidate


def main() -> None:
    # Line-buffer stdout/stderr so prompts and gate output stream out in real
    # time even when the script is tee'd or piped to a log file.
    try:
        sys.stdout.reconfigure(line_buffering=True)
        sys.stderr.reconfigure(line_buffering=True)
    except (AttributeError, ValueError):
        pass
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--max-iterations",
        type=int,
        default=0,
        help="Cap on total claude invocations (0 = unlimited).",
    )
    parser.add_argument(
        "--timeout",
        type=float,
        default=3600,
        help="Per-claude-call timeout in seconds (default: 3600s = 1 hour).",
    )
    parser.add_argument(
        "--max-budget-usd",
        type=float,
        default=20.0,
        dest="max_budget_usd",
        help=(
            "Per-dispatch spend cap passed to `claude --max-budget-usd` "
            "(default: $20). Set to 0 to disable."
        ),
    )
    parser.add_argument(
        "--port",
        type=str,
        default=None,
        metavar="PATH",
        help=(
            "Port exactly this upstream file (absolute or repo-relative). "
            "Runs repair → drive_port → repair and exits, instead of "
            "looping over random unported picks."
        ),
    )
    parser.add_argument(
        "--todo-only",
        action="store_true",
        help=(
            "Only drain TODO.yaml: repair, then pop one entry, then repeat "
            "until TODO.yaml is empty. Does not advance to porting random "
            "unported files. Exits 0 once the queue is empty."
        ),
    )
    parser.add_argument(
        "--model",
        type=str,
        default="sonnet",
        help=(
            "Model alias passed to `claude -p --model` for every dispatch "
            "(default: sonnet)."
        ),
    )
    parser.add_argument(
        "--clean",
        action="store_true",
        help="Run `cargo clean` at the start of each outer-loop iteration.",
    )
    parser.add_argument(
        "--finalize",
        action="store_true",
        help=(
            "Integration mode: once porting is complete, pick files from "
            "tests/{hypothesis,pbtkit}/ one at a time and dispatch agents "
            "to integrate them into the main test suite. Progress is tracked "
            "in FINALIZED.md. Incompatible with --port, --todo-only, and "
            "--max-workers>1."
        ),
    )
    parser.add_argument(
        "--dangerously-skip-permissions",
        action="store_true",
        dest="skip_permissions",
        help="Pass --dangerously-skip-permissions to each claude invocation.",
    )
    parser.add_argument(
        "--max-workers",
        type=int,
        default=1,
        dest="max_workers",
        help=(
            "Maximum concurrent porting workers (default: 1 = serial, "
            "original behaviour). When > 1, the porting phase spawns up "
            "to N subprocesses in git worktrees under .port-worktrees/. "
            "Each worker uses its own CARGO_TARGET_DIR (target-worker-I/) "
            "so builds don't serialize on Cargo file locks; expect "
            "several GB of extra disk per worker."
        ),
    )
    parser.add_argument(
        "--per-file-dispatches",
        type=int,
        default=12,
        dest="per_file_dispatches",
        help=(
            "Per-file dispatch budget for parallel workers (default: 12). "
            "When a worker hits this cap without all gates green, the "
            "supervisor dispatches a rescue agent that abandons the port "
            "by recording a SKIPPED/TODO entry. Ignored when "
            "--max-workers=1."
        ),
    )
    # --- hidden flags used by subprocess workers ---
    parser.add_argument(
        "--worker-mode",
        action="store_true",
        dest="worker_mode",
        help=argparse.SUPPRESS,
    )
    parser.add_argument(
        "--todo-worker-mode",
        action="store_true",
        dest="todo_worker_mode",
        help=argparse.SUPPRESS,
    )
    parser.add_argument(
        "--todo-payload",
        type=str,
        default=None,
        dest="todo_payload",
        help=argparse.SUPPRESS,
    )
    parser.add_argument(
        "--worktree",
        type=str,
        default=None,
        help=argparse.SUPPRESS,
    )
    parser.add_argument(
        "--supervisor-branch",
        type=str,
        default=None,
        dest="supervisor_branch",
        help=argparse.SUPPRESS,
    )
    parser.add_argument(
        "--log-prefix",
        type=str,
        default="",
        dest="log_prefix",
        help=argparse.SUPPRESS,
    )
    args = parser.parse_args()
    if args.port is not None and args.todo_only:
        parser.error("--port and --todo-only are mutually exclusive.")
    if args.finalize:
        if args.port is not None:
            parser.error("--finalize is incompatible with --port.")
        if args.todo_only:
            parser.error("--finalize is incompatible with --todo-only.")
        if args.max_workers > 1:
            parser.error("--finalize is incompatible with --max-workers>1.")
        if args.worker_mode:
            parser.error("--finalize is incompatible with --worker-mode.")
    if args.max_workers < 1:
        parser.error("--max-workers must be >= 1.")
    if args.worker_mode and args.todo_worker_mode:
        parser.error("--worker-mode and --todo-worker-mode are mutually exclusive.")
    if args.worker_mode:
        if not args.worktree:
            parser.error("--worker-mode requires --worktree.")
        if not args.port:
            parser.error("--worker-mode requires --port.")
        if not args.supervisor_branch:
            parser.error("--worker-mode requires --supervisor-branch.")
        if args.max_workers != 1:
            parser.error("--worker-mode is incompatible with --max-workers>1.")
    elif args.todo_worker_mode:
        if not args.worktree:
            parser.error("--todo-worker-mode requires --worktree.")
        if not args.todo_payload:
            parser.error("--todo-worker-mode requires --todo-payload.")
        if not args.supervisor_branch:
            parser.error("--todo-worker-mode requires --supervisor-branch.")
        if args.max_workers != 1:
            parser.error(
                "--todo-worker-mode is incompatible with --max-workers>1."
            )
    elif args.max_workers > 1:
        if args.port:
            parser.error("--max-workers>1 is incompatible with --port.")
        if args.todo_only:
            parser.error("--max-workers>1 is incompatible with --todo-only.")
    if args.per_file_dispatches < 1:
        parser.error("--per-file-dispatches must be >= 1.")

    # Use sccache as a shared compiler cache when available.  Workers inherit
    # this via `os.environ.copy()`, so a single sccache daemon serves all of
    # them and the supervisor, cutting repeated dependency compilations down to
    # cache hits.
    if shutil.which("sccache") and "RUSTC_WRAPPER" not in os.environ:
        os.environ["RUSTC_WRAPPER"] = "sccache"
        print("[port-loop] sccache found; setting RUSTC_WRAPPER=sccache for all builds.")

    state = IterCounter(
        args.max_iterations,
        args.timeout,
        args.model,
        max_budget_usd=args.max_budget_usd if args.max_budget_usd else None,
        skip_permissions=args.skip_permissions,
    )

    def maybe_clean() -> None:
        if args.clean:
            cargo_clean()

    if args.worker_mode:
        # Running inside a parallel-port worktree. cwd isn't used for
        # path resolution (REPO_ROOT comes from __file__, which is this
        # worktree's copy of the script), but we chdir anyway so any
        # accidental Path.cwd() usage picks up the worktree.
        os.chdir(args.worktree)
        _install_log_prefix(args.log_prefix)
        state.per_file_cap = args.per_file_dispatches
        picked = resolve_port_arg(args.port)
        destination = destination_for(picked)
        print(
            f"\n[port-loop] --worker-mode: targeting {picked} → "
            f"{destination} on branch {_current_branch()} (supervisor "
            f"branch {args.supervisor_branch}, per-file cap "
            f"{args.per_file_dispatches})."
        )
        try:
            repair(state)
            drive_port_worker(
                picked, destination, args.supervisor_branch, state,
            )
        except GateBudgetExhausted as e:
            print(f"\n[port-loop] --worker-mode: {e}; exiting 42 for rescue.")
            sys.exit(42)
        except RuntimeError as e:
            print(
                f"\n[port-loop] --worker-mode: post-rebase unrecoverable "
                f"({e}); exiting 43 for merge-rescue."
            )
            sys.exit(43)
        print(
            f"\n[port-loop] --worker-mode: green; exiting 0 after "
            f"{state.n} iteration(s)."
        )
        return

    if args.todo_worker_mode:
        # Single-shot: dispatch one claude agent on the supervisor's
        # pre-picked TODO entry, then exit. The supervisor cherry-picks
        # whatever commits the agent made onto the supervisor branch.
        os.chdir(args.worktree)
        _install_log_prefix(args.log_prefix)
        payload_path = Path(args.todo_payload)
        if not payload_path.is_file():
            sys.exit(
                f"[port-loop] --todo-worker-mode: payload file not found: "
                f"{payload_path}"
            )
        payload = yaml.safe_load(payload_path.read_text())
        entry = payload["entry"]
        attempts = int(payload["attempts"])
        remaining = int(payload["remaining"])
        title = str(entry.get("title", "")) or format_todo(entry).splitlines()[0]
        if attempts >= 4:
            print(
                f"\n[port-loop] --todo-worker-mode: recovery dispatch for "
                f"{title!r} (attempt {attempts + 1})."
            )
            prompt = TODO_RECOVERY_PROMPT.format(
                entry=format_todo(entry), attempts=attempts
            )
        else:
            print(
                f"\n[port-loop] --todo-worker-mode: dispatch for {title!r} "
                f"(attempt {attempts + 1}/5)."
            )
            prompt = TODO_PROMPT.format(
                entry=format_todo(entry), remaining=remaining
            )
        state.dispatch(prompt)
        print(
            f"\n[port-loop] --todo-worker-mode: done after "
            f"{state.n} iteration(s)."
        )
        return

    if args.port is not None:
        picked = resolve_port_arg(args.port)
        destination = destination_for(picked)
        print(
            f"\n[port-loop] --port: targeting {picked} → {destination}; "
            f"running pre-repair, sub-loop, then post-repair."
        )
        maybe_clean()
        repair(state)
        drive_port(picked, destination, state)
        repair(state)
        print(f"\n[port-loop] --port done after {state.n} iteration(s).")
        return

    if args.todo_only:
        while True:
            maybe_hot_reload()
            maybe_clean()
            repair(state)
            dispatched, pushed = sync_with_origin(state)
            if dispatched:
                continue
            if drive_pr_ci(state, just_pushed=pushed):
                continue
            if not drive_todos(state):
                print(
                    f"\n[port-loop] --todo-only: TODO.yaml empty; done after "
                    f"{state.n} iteration(s)."
                )
                return

    if args.finalize:
        while True:
            maybe_hot_reload()
            maybe_clean()
            repair(state)
            dispatched, pushed = sync_with_origin(state)
            if dispatched:
                continue
            if drive_pr_ci(state, just_pushed=pushed):
                continue
            if drive_todos(state):
                continue
            pool = finalize_pool()
            if not pool:
                print(
                    f"\n[port-loop] --finalize: all files integrated; "
                    f"done after {state.n} iteration(s)."
                )
                break
            picked = random.choice(pool)
            print(
                f"\n[port-loop] --finalize: {len(pool)} file(s) remain; "
                f"picked {picked.relative_to(REPO_ROOT)} (random)."
            )
            drive_finalize_file(picked, state)
        repair(state, run_server_tests=True)
        return

    while True:
        maybe_hot_reload()
        maybe_clean()
        repair(state)

        dispatched, pushed = sync_with_origin(state)
        if dispatched:
            continue
        if drive_pr_ci(state, just_pushed=pushed):
            continue

        # Serial mode (max-workers=1): drain TODOs in the foreground
        # before porting, preserving the original behaviour. Parallel
        # mode skips this — the dedicated TODO worker inside the pool
        # drains TODO.yaml concurrently with port workers.
        if args.max_workers == 1 and drive_todos(state):
            continue

        pool = unported_pool()
        todos_pending = read_todos() if args.max_workers > 1 else []

        if not pool and not todos_pending:
            print(
                f"\n[port-loop] TODO.yaml empty and every upstream file is "
                f"ported or skipped; done after {state.n} iteration(s)."
            )
            break

        if args.max_workers > 1:
            print(
                f"\n[port-loop] {len(pool)} file(s) and "
                f"{len(todos_pending)} TODO entry(ies) remain; entering "
                f"parallel pool with max-workers={args.max_workers}."
            )
            drive_port_pool(state, args)
            continue

        picked = random.choice(pool)
        destination = destination_for(picked)
        print(
            f"\n[port-loop] {len(pool)} files remain; picked {picked} "
            f"→ {destination} (random)."
        )
        drive_port(picked, destination, state)

    repair(state, run_server_tests=True)


if __name__ == "__main__":
    main()
