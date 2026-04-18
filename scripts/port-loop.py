#!/usr/bin/env python3
"""Self-driving loop that runs gates, then picks an upstream test file to port.

Each iteration:
  1. `just lint`
  2. `cargo test`
  3. `HEGEL_SERVER_COMMAND=/bin/false cargo test --features native`
  4. working tree clean (`git status --porcelain` empty)
  5. if all pass, pick a random unported upstream file and dispatch claude to port it

On the first failing gate, claude is invoked with a short fix prompt and the
loop restarts. When every upstream file is accounted for (ported or in
SKIPPED.md) and the gates all pass, the loop exits 0.
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


# ---- prompts (tune freely) ---------------------------------------------------

COMMON_SYSTEM_PROMPT = """\
You are being driven by scripts/port-loop.py, a non-interactive loop that
calls you with one focused task per invocation. Do the task, commit, and
exit. The loop re-runs the gates (just lint, cargo test, native-mode tests,
clean tree) after you return, so a partial fix is fine — the next
invocation will pick up from wherever the gates next fail.

Ground rules:
- Work in TDD order when fixing bugs (regression test first).
- Commit every focused change with a descriptive message. Never --amend,
  never --no-verify.
- If a port is truly unportable, add its filename to SKIPPED.md under the
  right section with a one-line rationale and commit, rather than leaving
  a stub.
- Read .claude/skills/porting-tests/SKILL.md before porting a file.
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
skill.

If the file has no tests that can be ported to hegel-rust, instead add
`{name}` to SKIPPED.md under the appropriate section with a one-line
rationale. Either way, commit the result.
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


def gate_clean_tree() -> tuple[bool, str]:
    print("\n$ git status --porcelain")
    result = subprocess.run(
        ["git", "status", "--porcelain"],
        cwd=REPO_ROOT,
        capture_output=True,
        text=True,
        check=False,
    )
    out = result.stdout + result.stderr
    if out:
        sys.stdout.write(out)
        sys.stdout.flush()
    return result.returncode == 0 and not result.stdout.strip(), out


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


def main() -> int:
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
    args = parser.parse_args()

    iteration = 0
    while True:
        iteration += 1
        if args.max_iterations > 0 and iteration > args.max_iterations:
            print(
                f"\n[port-loop] hit --max-iterations={args.max_iterations}; stopping."
            )
            return 1
        print(f"\n{'#' * 72}\n# iteration {iteration}\n{'#' * 72}")

        ok, out = gate_lint()
        if not ok:
            dispatch_claude(LINT_FIX_PROMPT, gate_output=out, timeout=args.timeout)
            continue

        ok, out = gate_server_tests()
        if not ok:
            dispatch_claude(
                SERVER_TEST_FIX_PROMPT, gate_output=out, timeout=args.timeout
            )
            continue

        ok, out = gate_native_tests()
        if not ok:
            dispatch_claude(
                NATIVE_TEST_FIX_PROMPT, gate_output=out, timeout=args.timeout
            )
            continue

        ok, out = gate_clean_tree()
        if not ok:
            dispatch_claude(COMMIT_PROMPT, gate_output=out, timeout=args.timeout)
            continue

        pool = unported_pool()
        if not pool:
            print(
                f"\n[port-loop] all gates pass and every upstream file is ported "
                f"or skipped. Exiting after {iteration} iteration(s)."
            )
            return 0

        picked = random.choice(pool)
        destination = destination_for(picked)
        print(
            f"\n[port-loop] {len(pool)} files remain; picked {picked} "
            f"→ {destination} (random)."
        )
        prompt = PORT_PROMPT.format(
            path=picked, destination=destination, name=picked.name
        )
        dispatch_claude(prompt, gate_output=None, timeout=args.timeout)


if __name__ == "__main__":
    sys.exit(main())
