#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = ["pyyaml>=6"]
# ///
"""Self-driving loop that runs gates, clears TODOs, then ports upstream tests.

Outer loop, each iteration:
  1. repair(): `just format` + `cargo clippy --fix` (auto-committed if the
     tree was clean before), then `just lint`, `cargo test`,
     `HEGEL_SERVER_COMMAND=/bin/false cargo test --features native`, clean
     tree — each as a gate that dispatches claude on failure.
  2. If `TODO.yaml` has any entries, pop the first one and dispatch claude
     to clear it, then continue the outer loop (repair runs again before
     the next action).
  3. If no TODOs, pick a random unported upstream file and enter the port
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
import json
import os
import random
import re
import subprocess
import sys
import threading
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

PORT_REVIEW_PROMPT = """\
Review the port of {path} → {destination}. The gate chain (destination
exists, has `#[test]` attributes, server-mode tests pass, native-mode
tests pass, working tree clean) is currently green. Below is the list
of commits made during this sub-loop ({start_sha}..HEAD).

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
- Did this port surface any new Python→Rust translation, pattern, or
  gotcha that isn't already documented under
  `.claude/skills/porting-tests/` (SKILL.md, references/api-mapping.md,
  references/pbtkit-overview.md, references/hypothesis-overview.md)?
  If so, add a terse entry in a commit.

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


# ---- gate helpers ------------------------------------------------------------


REPO_ROOT = Path(__file__).resolve().parent.parent
PBTKIT_DIR = REPO_ROOT / "resources" / "pbtkit" / "tests"
HYPOTHESIS_DIR = (
    REPO_ROOT / "resources" / "hypothesis" / "hypothesis-python" / "tests" / "cover"
)


def run_gate(cmd: list[str], *, env: dict[str, str] | None = None) -> tuple[int, str]:
    """Run a gate command, stream output live, return (exit_code, captured_output)."""
    print(f"\n$ {' '.join(cmd)}")
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
    for line in proc.stdout:
        sys.stdout.write(line)
        sys.stdout.flush()
        captured.append(line)
    proc.wait()
    return proc.returncode, "".join(captured)


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


def gate_lint() -> tuple[bool, str]:
    code, out = run_gate(["just", "lint"])
    return code == 0, out


def gate_server_tests() -> tuple[bool, str]:
    code, out = run_gate(["cargo", "test"])
    return code == 0, out


def gate_native_tests() -> tuple[bool, str]:
    env = os.environ.copy()
    env["HEGEL_SERVER_COMMAND"] = "/bin/false"
    code, out = run_gate(["cargo", "test", "--features", "native"], env=env)
    return code == 0, out


def gate_module_server(kind: str, module: str) -> tuple[bool, str]:
    code, out = run_gate(["cargo", "test", "--test", kind, module])
    return code == 0, out


def gate_module_native(kind: str, module: str) -> tuple[bool, str]:
    env = os.environ.copy()
    env["HEGEL_SERVER_COMMAND"] = "/bin/false"
    code, out = run_gate(
        ["cargo", "test", "--features", "native", "--test", kind, module],
        env=env,
    )
    return code == 0, out


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
                summary = _tool_summary(name, block.get("input") or {})
                print(f"[claude] → {name}({summary})", flush=True)
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
    prompt: str, *, gate_output: str | None, timeout: float | None
) -> None:
    full_prompt = prompt
    if gate_output is not None:
        full_prompt += f"\n\nGate output:\n{gate_output}"
    print("\n" + "=" * 72)
    print("Dispatching claude with prompt:")
    print("-" * 72)
    print(full_prompt)
    print("=" * 72, flush=True)

    proc = subprocess.Popen(
        [
            "claude",
            "-p",
            "--dangerously-skip-permissions",
            "--model",
            "opus",
            "--output-format",
            "stream-json",
            "--verbose",
            "--append-system-prompt",
            COMMON_SYSTEM_PROMPT,
            full_prompt,
        ],
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


# ---- main loop ---------------------------------------------------------------


class IterCounter:
    """Tracks and caps total claude dispatches across outer and sub-loops."""

    def __init__(self, max_iterations: int, timeout: float | None) -> None:
        self.n = 0
        self.max = max_iterations
        self.timeout = timeout

    def dispatch(self, prompt: str, *, gate_output: str | None = None) -> None:
        """Dispatch claude, or exit 0 if the iteration cap is already hit."""
        if self.max > 0 and self.n >= self.max:
            print(
                f"\n[port-loop] hit --max-iterations={self.max}; stopping."
            )
            sys.exit(0)
        self.n += 1
        print(f"\n{'#' * 72}\n# iteration {self.n}\n{'#' * 72}")
        dispatch_claude(prompt, gate_output=gate_output, timeout=self.timeout)


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
                state.dispatch(
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
        ok, out = gate_module_server(kind, module)
        if not ok:
            state.dispatch(
                PORT_TEST_FIX_SERVER_PROMPT.format(**fmt_args), gate_output=out
            )
            any_dispatched = True

        # Step 4: module's native-mode tests must pass.
        ok, out = gate_module_native(kind, module)
        if not ok:
            state.dispatch(
                PORT_TEST_FIX_NATIVE_PROMPT.format(**fmt_args), gate_output=out
            )
            any_dispatched = True

        # Step 5: tree must be clean.
        ok, out = gate_clean_tree()
        if not ok:
            state.dispatch(
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
        # there's nothing to review.
        current_sha = git_head()
        if current_sha == start_sha:
            print(
                f"\n[port-loop] {destination} green with no new commits; "
                f"sub-loop done."
            )
            break

        # Step 6: dispatch a review of the commits made during this port.
        print(
            f"\n[port-loop] {destination} ported and green; "
            f"dispatching review of {start_sha[:12]}..HEAD."
        )
        log = git_log(f"{start_sha}..HEAD")
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


def repair(state: IterCounter) -> None:
    any_failures = True
    while any_failures:
        any_failures = False
        apply_auto_fixes()
        ok, out = gate_lint()
        if not ok:
            any_failures = True
            state.dispatch(LINT_FIX_PROMPT, gate_output=out)

        ok, out = gate_server_tests()
        if not ok:
            state.dispatch(SERVER_TEST_FIX_PROMPT, gate_output=out)
            any_failures = True

        ok, out = gate_native_tests()
        if not ok:
            state.dispatch(NATIVE_TEST_FIX_PROMPT, gate_output=out)
            any_failures = True
        if any_failures:
            continue
        ok, out = gate_clean_tree()
        if not ok:
            state.dispatch(COMMIT_PROMPT, gate_output=out)
            any_failures = True


def drive_todos(state: IterCounter) -> bool:
    """Pop one TODO entry if any are pending. Returns True iff dispatched.

    When dispatching cleared the last entry in TODO.yaml, runs a full
    `repair()` before returning so the outer loop sees a fresh green
    baseline before switching to porting new tests.
    """
    todos = read_todos()
    if not todos:
        return False
    first = todos[0]
    remaining = len(todos) - 1
    print(
        f"\n[port-loop] {len(todos)} TODO entry(ies) pending; dispatching "
        f"first."
    )
    state.dispatch(TODO_PROMPT.format(entry=format_todo(first), remaining=remaining))
    if remaining == 0 and not read_todos():
        print(
            f"\n[port-loop] last TODO cleared; running a full repair before "
            f"moving on to porting."
        )
        repair(state)
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
    args = parser.parse_args()
    if args.port is not None and args.todo_only:
        parser.error("--port and --todo-only are mutually exclusive.")
    state = IterCounter(args.max_iterations, args.timeout)

    if args.port is not None:
        picked = resolve_port_arg(args.port)
        destination = destination_for(picked)
        print(
            f"\n[port-loop] --port: targeting {picked} → {destination}; "
            f"running pre-repair, sub-loop, then post-repair."
        )
        repair(state)
        drive_port(picked, destination, state)
        repair(state)
        print(f"\n[port-loop] --port done after {state.n} iteration(s).")
        return

    if args.todo_only:
        while True:
            repair(state)
            if not drive_todos(state):
                print(
                    f"\n[port-loop] --todo-only: TODO.yaml empty; done after "
                    f"{state.n} iteration(s)."
                )
                return

    while True:
        repair(state)

        if drive_todos(state):
            continue

        pool = unported_pool()
        if not pool:
            print(
                f"\n[port-loop] TODO.yaml empty and every upstream file is "
                f"ported or skipped; done after {state.n} iteration(s)."
            )
            return

        picked = random.choice(pool)
        destination = destination_for(picked)
        print(
            f"\n[port-loop] {len(pool)} files remain; picked {picked} "
            f"→ {destination} (random)."
        )
        drive_port(picked, destination, state)


if __name__ == "__main__":
    main()
