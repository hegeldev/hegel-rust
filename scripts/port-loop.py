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
  4. If the upstream PR (hegeldev/hegel-rust#188) has completed CI as
     failed, dispatch a specialised agent to fix CI and restart the
     iteration before doing any other work.
  5. If `TODO.yaml` has any entries, pop the first one and dispatch claude
     to clear it, then continue the outer loop (repair runs again before
     the next action).
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
import subprocess
import sys
import threading
import time
from pathlib import Path

import yaml


# ---- prompts (tune freely) ---------------------------------------------------

COMMON_SYSTEM_PROMPT = """\
You are being driven by scripts/port-loop.py, a non-interactive loop that
calls you with one focused task per invocation. Do the task, commit, and
exit. The loop re-runs its gates after you return, so a partial fix is
fine — the next invocation will pick up from wherever the gates next
fail.

Ground rules:
- Work in TDD order when fixing bugs (regression test first).
- Commit every focused change with a descriptive message. Never --amend,
  never --no-verify.
- Read .claude/skills/porting-tests/SKILL.md before porting or reviewing
  a port.
- Read .claude/skills/implementing-native/SKILL.md before adding or
  extending code under src/native/ (including filling in a todo!()
  stub, or native-gating a test that needs new engine support). The
  key rule: consult pbtkit (resources/pbtkit/src/pbtkit/) first, then
  Hypothesis (resources/hypothesis/hypothesis-python/src/hypothesis/
  internal/) only where pbtkit is insufficient. The native engine is
  a port — match upstream semantics rather than reinventing them.
- As you port, keep the skills current. If you figure out how to port
  something in a way not already covered by the porting-tests skill
  (or its references under .claude/skills/porting-tests/references/)
  or the implementing-native skill — a Python→Rust translation that's
  missing from the cheat sheet, a non-obvious pattern, a gotcha —
  update the relevant file in the same commit. Don't duplicate things
  that are already documented.

Skip vs. port policy (applies to every port-related task):

- Add a file to SKIPPED.md ONLY when its tests rely on *public API* that
  has no hegel-rust counterpart: Python-specific facilities (pickle,
  __repr__, sys.modules, Python syntax, dunder access) or integrations
  with other Python libraries (numpy, pandas, django, attrs, redis).

- "Has no Rust counterpart" is NOT on its own a valid reason to skip. It
  is the reason to PORT. Tests on internal APIs (pbtkit / Hypothesis
  engine internals — `PbtkitState`, `ChoiceNode`, `ConjectureRunner`,
  `SHRINK_PASSES`, `TC.for_choices`, `to_index`/`from_index`, database
  serialization tags, span introspection, etc.) exist to pin down
  behaviour that `src/native/` needs to match. Port them. If the
  corresponding native feature doesn't exist yet:
  * native-gate the test with `#[cfg(feature = "native")]` (or
    `#![cfg(feature = "native")]` at file top if every test in it is
    native-only),
  * add the missing feature under `src/native/`. If it's easy,
    implement it properly. If it's hard, stub the function body with
    `todo!("...")` so a later fixer-task invocation picks it up.
  * the test itself MUST compile cleanly in both modes. `todo!()`
    belongs in the source code, never in the test body. "Too complex
    to port" is not a valid reason to skip — that's exactly the
    native-gated-plus-source-stub case.

- Do NOT skip tests on the grounds that "hegel-rust already has an
  equivalent test elsewhere", "this is covered by tests/foo.rs", or
  "this looks redundant". Redundancy is fine. Incorrectly skipping a
  test is much worse than porting something a second time. A later
  rationalisation pass will deduplicate; don't pre-empt it.
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

PR_CI_FIX_PROMPT = """\
CI on https://github.com/{repo}/pull/{pr} has completed with failures,
and the port loop is parked on that until it's fixed. Output of `gh
pr checks {pr} --repo {repo}` is included below.

Steps:
1. Identify which checks failed from the output below.
2. Pull the failing-run logs:
      gh run view <run-id> --log-failed --repo {repo}
   (the run-id is in the "link" column of `gh pr checks`).
3. Find the root cause and fix it on the PR's head branch:
   - If the PR tracks the currently-checked-out branch (`git rev-parse
     --abbrev-ref HEAD` matches the PR's headRefName), make the fix in
     place: commit and `git push` here.
   - Otherwise: `gh pr checkout {pr}`, fix, commit, push, then
     `git switch -` to return to the original branch before exiting.
4. Do NOT paper over failures with `--no-verify`, `#[ignore]`,
   SKIPPED.md entries, or by removing the failing test. Fix the root
   cause.
5. Do NOT `git push --force` unless the PR history truly needs
   rewriting (use `--force-with-lease` if so).

One focused commit is fine. The next port-loop iteration re-checks PR
CI; if it's still red after its next run, you'll be dispatched again
with fresh output.
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
in the system prompt) `{name}` is in SKIPPED.md. Make one focused commit
toward that goal.
"""

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

Per the skip policy in the system prompt, missing native-mode engine
features are NOT a reason to add this file to SKIPPED.md. Instead:
native-gate the affected test(s) with `#[cfg(feature = "native")]` and
add the missing feature under `src/native/` — stubbed with `todo!()` if
it's too large to implement in one focused commit. The test itself
must compile in both modes; the `todo!()` goes in the source.

When you do implement (or stub) the missing feature, read
.claude/skills/implementing-native/SKILL.md first: consult pbtkit
(`resources/pbtkit/src/pbtkit/`) as the primary reference, and
Hypothesis (`resources/hypothesis/hypothesis-python/src/hypothesis/
internal/`) only where pbtkit is insufficient.
"""

PORT_COMMIT_PROMPT = """\
Continuing port {path} → {destination}. Filtered tests pass in both
server and native mode, but the working tree is dirty. `git status
--porcelain` output below. Make a focused commit.
"""

PORT_MISSING_TESTS_PROMPT = """\
Continuing port {path} → {destination}. The destination file exists
but contains no `#[test]` attribute — either the port is incomplete
or stubbed out. Add the ported tests and commit. Review the skip
policy in the system prompt before routing this file to SKIPPED.md;
it is strict.
"""

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

PORT_REVIEW_PROMPT = """\
Review the port of {path} → {destination}. The gate chain (destination
exists, has `#[test]` attributes, server-mode tests pass, native-mode
tests pass, working tree clean) is currently green, and a skill-update
reflection pass has already run. Below is the list of commits made
during this sub-loop ({start_sha}..HEAD).

Read the upstream file ({path}), the ported file ({destination}), and
the commits under review. Then evaluate honestly, applying the skip
policy from the system prompt:

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
"""

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


# ---- gate helpers ------------------------------------------------------------


REPO_ROOT = Path(__file__).resolve().parent.parent
PBTKIT_DIR = REPO_ROOT / "resources" / "pbtkit" / "tests"
HYPOTHESIS_DIR = (
    REPO_ROOT / "resources" / "hypothesis" / "hypothesis-python" / "tests" / "cover"
)

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
_PASSING_TEST_RE = re.compile(r"^\s*test \S+ \.\.\. ok\s*$")
_RUNNING_PREAMBLE_RE = re.compile(r"^\s*running \d+ tests?\s*$")


def strip_build_noise(output: str) -> str:
    """Remove cargo progress + passing-test lines from gate output.

    Keeps failing tests, panic/backtrace, slow-test warnings, compile
    errors, clippy output, test-result summaries, and everything else
    that could help an agent diagnose the failure. Collapses runs of
    blank lines so stripping doesn't leave huge gaps.
    """
    kept: list[str] = []
    prev_blank = False
    for line in output.splitlines():
        stripped = line.lstrip()
        if any(stripped.startswith(p) for p in _CARGO_PROGRESS_PREFIXES):
            continue
        if _PASSING_TEST_RE.match(line):
            continue
        if _RUNNING_PREAMBLE_RE.match(line):
            continue
        if not line.strip():
            if prev_blank:
                continue
            prev_blank = True
        else:
            prev_blank = False
        kept.append(line)
    return "\n".join(kept)

# Hot-reload bookkeeping: captured once at import, checked at the top of
# each main-loop iteration. If this script's mtime changes mid-run
# (e.g. during a `git pull` or while the user is iterating on the
# driver itself), re-exec into the new version with the original argv
# so the long-running loop doesn't need to be restarted by hand.
SCRIPT_PATH = Path(__file__).resolve()
SCRIPT_MTIME_AT_STARTUP = SCRIPT_PATH.stat().st_mtime
ORIGINAL_ARGV: list[str] = list(sys.argv)


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
    return result.stdout


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
            stem = path.stem.removeprefix("test_")
            if stem in ported:
                continue
            pool.append(path)
    return pool


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
                # Skip internal thinking blocks in live output.
                pass
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
    resume_session: str | None = None,
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
    print("\n" + "=" * 72)
    if resume_session is not None:
        print(f"Resuming claude session {resume_session[:12]}… with prompt:")
    else:
        print("Dispatching claude with prompt:")
    print("-" * 72)
    print(full_prompt)
    print("=" * 72, flush=True)

    cmd = [
        "claude",
        "-p",
        "--dangerously-skip-permissions",
        "--model",
        model,
        "--output-format",
        "stream-json",
        "--verbose",
    ]
    if resume_session is not None:
        # Resuming carries forward the original session's system prompt
        # and history, so don't re-append. The follow-up task goes in as
        # the final positional prompt argument.
        cmd += ["--resume", resume_session]
    else:
        cmd += ["--append-system-prompt", COMMON_SYSTEM_PROMPT]
    cmd.append(full_prompt)

    proc = subprocess.Popen(
        cmd,
        cwd=REPO_ROOT,
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

    session_id: str | None = None
    assert proc.stdout is not None
    try:
        for raw in proc.stdout:
            line = raw.rstrip("\n")
            if not line:
                continue
            try:
                evt = json.loads(line)
            except json.JSONDecodeError:
                print(f"[claude:raw] {line}", flush=True)
                continue
            if evt.get("type") == "system" and evt.get("subtype") == "init":
                sid = evt.get("session_id")
                if isinstance(sid, str):
                    session_id = sid
            try:
                _print_event(evt)
            except Exception as e:
                print(f"[port-loop] event-format error: {e}", flush=True)
    finally:
        if timer is not None:
            timer.cancel()
        proc.wait()
        if timed_out:
            print(f"\n[port-loop] claude timed out after {timeout}s; continuing.")

    return session_id, proc.returncode


# ---- main loop ---------------------------------------------------------------


class IterCounter:
    """Tracks and caps total claude dispatches across outer and sub-loops.

    Also remembers the session_id of the most recent dispatch so that
    follow-up prompts ("commit the dirty tree you just produced") can
    `--resume` that same session instead of spawning a context-free
    fresh agent that would have to re-derive the diff.
    """

    def __init__(
        self, max_iterations: int, timeout: float | None, model: str
    ) -> None:
        self.n = 0
        self.max = max_iterations
        self.timeout = timeout
        self.model = model
        self.last_session_id: str | None = None

    def _check_cap(self) -> None:
        if self.max > 0 and self.n >= self.max:
            print(f"\n[port-loop] hit --max-iterations={self.max}; stopping.")
            sys.exit(0)

    def dispatch(self, prompt: str, *, gate_output: str | None = None) -> None:
        """Dispatch a fresh claude session, or exit 0 if the cap is hit."""
        self._check_cap()
        self.n += 1
        print(f"\n{'#' * 72}\n# iteration {self.n}\n{'#' * 72}")
        sid, _code = dispatch_claude(
            prompt,
            gate_output=gate_output,
            timeout=self.timeout,
            model=self.model,
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
            resume_session=previous,
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
        # Exit A: upstream is now in SKIPPED.md.
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

        any_dispatched = False

        # Step 1: destination must exist.
        if not destination.exists():
            state.dispatch(PORT_PROMPT.format(**fmt_args))
            any_dispatched = True

        # Step 2: destination must contain at least one #[test].
        if not destination_has_tests(destination):
            state.dispatch(PORT_MISSING_TESTS_PROMPT.format(**fmt_args))
            any_dispatched = True

        # Step 3: module's server-mode tests must pass.
        ok, out, perf = gate_module_server(kind, module)
        if not ok:
            prompt = test_fix_prompt_for(
                perf, PORT_TEST_FIX_SERVER_PROMPT.format(**fmt_args)
            )
            state.dispatch(prompt, gate_output=strip_build_noise(out))
            any_dispatched = True

        # Step 4: module's native-mode tests must pass.
        ok, out, perf = gate_module_native(kind, module)
        if not ok:
            prompt = test_fix_prompt_for(
                perf, PORT_TEST_FIX_NATIVE_PROMPT.format(**fmt_args)
            )
            state.dispatch(prompt, gate_output=strip_build_noise(out))
            any_dispatched = True

        # Step 5: tree must be clean.
        ok, out = gate_clean_tree()
        if not ok:
            state.resume_last(
                PORT_COMMIT_PROMPT.format(**fmt_args), gate_output=out
            )
            any_dispatched = True

        if any_dispatched:
            # Progress was made but the port isn't green yet; fall back to
            # the outer loop so lint/full-test gates get a chance before we
            # retry this file.
            print(
                f"\n[port-loop] sub-loop iteration made progress; "
                f"returning to outer loop."
            )
            break

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

    attempts_map = _load_todo_attempts()
    hashes = [_todo_hash(t) for t in todos]
    attempts_map = _prune_todo_attempts(attempts_map, set(hashes))

    # Rank (attempts_so_far, original_index) ascending.
    ranked = sorted(
        range(len(todos)),
        key=lambda i: (attempts_map.get(hashes[i], 0), i),
    )
    # Skip entries that have exhausted the budget (5 normal + 1 recovery).
    pickable = [i for i in ranked if attempts_map.get(hashes[i], 0) < 6]
    if not pickable:
        print(
            f"\n[port-loop] all {len(todos)} TODO entry(ies) have exhausted "
            f"their attempt budget; leaving them in place and skipping to "
            f"porting."
        )
        return False

    idx = pickable[0]
    entry = todos[idx]
    entry_hash = hashes[idx]
    title = str(entry.get("title", "")) or format_todo(entry).splitlines()[0]
    attempts = attempts_map.get(entry_hash, 0)
    remaining = len(todos) - 1

    attempts_map[entry_hash] = attempts + 1
    _save_todo_attempts(attempts_map)

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


def gate_sync_with_origin() -> tuple[bool, str, bool]:
    """Fetch, rebase onto origin/main, push. Returns (ok, output, pushed).

    Called only after local gates are green and the tree is clean. Does
    nothing destructive: always uses `--force-with-lease`, never
    `--force`. Refuses to operate on the `main`/`master` branch itself
    (the loop should run on a feature branch).

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

    if origin_branch is None:
        push = _run_git(["push", "--set-upstream", "origin", "HEAD"])
    else:
        push = _run_git(["push", "--force-with-lease", "origin", "HEAD"])
    _record(push)
    if push.returncode != 0:
        return False, "".join(combined), False

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


# ---- PR CI gate -------------------------------------------------------------


def _pr_check_status() -> tuple[str, str, str]:
    """Return (status, summary, detail) for the tracked PR's CI.

    `status` is one of `"skip"` (can't tell / PR closed / no checks),
    `"pending"`, `"success"`, `"failure"`. `summary` is a short label
    for logs. `detail` is the raw `gh pr checks` output suitable for
    handing to a fix agent (empty for statuses other than `"failure"`).
    """
    try:
        result = subprocess.run(
            [
                "gh",
                "pr",
                "checks",
                str(PR_NUMBER),
                "--repo",
                PR_REPO,
                "--json",
                "name,bucket,state,link,workflow",
            ],
            capture_output=True,
            text=True,
            timeout=60,
            check=False,
        )
    except (subprocess.TimeoutExpired, FileNotFoundError) as e:
        return "skip", f"gh unavailable: {e}", ""
    if result.returncode != 0:
        return "skip", f"gh pr checks failed: {result.stderr.strip()}", ""
    try:
        checks = json.loads(result.stdout or "[]")
    except json.JSONDecodeError:
        return "skip", "gh pr checks returned non-JSON", ""
    if not checks:
        return "skip", "no checks reported yet", ""

    buckets = [c.get("bucket", "") for c in checks]
    if any(b == "pending" for b in buckets):
        n = sum(1 for b in buckets if b == "pending")
        return "pending", f"{n}/{len(checks)} checks still running", ""
    failing = [
        c.get("name", "?")
        for c in checks
        if c.get("bucket") in ("fail", "cancel")
    ]
    if failing:
        # Grab the human-readable form for the agent prompt.
        detail_proc = subprocess.run(
            ["gh", "pr", "checks", str(PR_NUMBER), "--repo", PR_REPO],
            capture_output=True,
            text=True,
            check=False,
        )
        detail = detail_proc.stdout + detail_proc.stderr
        return (
            "failure",
            f"{len(failing)} failing: {', '.join(failing)}",
            detail,
        )
    return "success", f"all {len(checks)} checks passing", ""


def gate_pr_ci() -> tuple[bool, str]:
    """Returns (ok, detail). ok=False only on completed-failure."""
    print(f"\n$ gh pr checks {PR_NUMBER} --repo {PR_REPO}")
    status, summary, detail = _pr_check_status()
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
        status, summary, _ = _pr_check_status()
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


def drive_pr_ci(state: IterCounter) -> bool:
    """If PR CI has completed as failed, dispatch the fix agent and wait.

    After dispatching, blocks until a new CI cycle completes (`gh pr
    checks --watch`) so the outer loop doesn't race ahead to TODOs while
    the fix push is still being verified by CI. If the new cycle is
    still failing, the next outer iteration observes that and dispatches
    again; if it's green, the outer loop proceeds to TODOs / porting.

    Returns True iff an agent was dispatched (caller should continue
    the outer loop).
    """
    ok, detail = gate_pr_ci()
    if ok:
        return False
    pre_sha = _rev_parse("HEAD")
    state.dispatch(
        PR_CI_FIX_PROMPT.format(repo=PR_REPO, pr=PR_NUMBER),
        gate_output=detail,
    )
    if _rev_parse("HEAD") == pre_sha:
        print(
            "[port-loop] PR CI fix agent produced no new commit; "
            "skipping --watch (outer loop will re-evaluate)."
        )
        return True
    if _wait_for_new_pr_checks():
        _watch_pr_ci()
    else:
        print(
            "[port-loop] PR CI: no pending checks appeared after push; "
            "outer loop will re-evaluate."
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
        default=600,
        help="Per-claude-call timeout in seconds (default: 600s = 10 minutes).",
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
    args = parser.parse_args()
    if args.port is not None and args.todo_only:
        parser.error("--port and --todo-only are mutually exclusive.")
    state = IterCounter(args.max_iterations, args.timeout, args.model)

    if args.port is not None:
        picked = resolve_port_arg(args.port)
        destination = destination_for(picked)
        print(
            f"\n[port-loop] --port: targeting {picked} → {destination}; "
            f"running pre-repair, sub-loop, then post-repair."
        )
        cargo_clean()
        repair(state)
        drive_port(picked, destination, state)
        repair(state)
        print(f"\n[port-loop] --port done after {state.n} iteration(s).")
        return

    if args.todo_only:
        while True:
            maybe_hot_reload()
            cargo_clean()
            repair(state)
            dispatched, pushed = sync_with_origin(state)
            if dispatched:
                continue
            if not pushed and drive_pr_ci(state):
                continue
            if not drive_todos(state):
                print(
                    f"\n[port-loop] --todo-only: TODO.yaml empty; done after "
                    f"{state.n} iteration(s)."
                )
                return

    while True:
        maybe_hot_reload()
        cargo_clean()
        repair(state)

        dispatched, pushed = sync_with_origin(state)
        if dispatched:
            continue
        if not pushed and drive_pr_ci(state):
            continue

        if drive_todos(state):
            continue

        pool = unported_pool()
        if not pool:
            print(
                f"\n[port-loop] TODO.yaml empty and every upstream file is "
                f"ported or skipped; done after {state.n} iteration(s)."
            )
            break

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
