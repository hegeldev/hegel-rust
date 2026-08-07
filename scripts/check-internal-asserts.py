#!/usr/bin/env python3
"""Forbid std assertion macros in src/, and panicking internal-error
raises in hegel-c/src/.

A plain `assert!` (or `assert_eq!` / `assert_ne!` / `debug_assert*!`) that
fires inside a running test body unwinds exactly like a failing property:
the engine classifies it as a counterexample, spends the post-bug window
and the shrink budget "minimizing" a framework bug, and reports it with a
reproducer blob.

Instead:

- Internal invariants (bugs in hegel itself) must use the
  `hegel_internal_assert!` family from `src/control.rs`. In the frontend
  (`src/`) a violated invariant panics out of the run with a bug-report
  message; in the engine (`hegel-c/src/`) it returns an `InternalError`
  `Err` threaded through the containing call graph — the engine must never
  deliberately panic, so `hegel-c/src/control.rs` must not contain a
  `panic!` for the macros to expand to.
- User-facing argument validation must use `invalid_argument!` from
  `src/test_case.rs` (frontend) or return `EngineError::InvalidArgument`
  (engine).

Doc comments and `//` comments are exempt (doc examples legitimately show
`assert!` in user test bodies). Test code lives under `tests/`, which this
check does not scan.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

# `(?<![\w!])` keeps `hegel_internal_assert!` (and friends) from matching
# their own suffixes.
ASSERT_MACRO = re.compile(r"(?<![\w!])(?:debug_)?assert(?:_eq|_ne)?!\s*\(")

# The engine's internal-error funnel must stay Err-returning: its control.rs
# must expand the hegel_internal_* macros to `return Err(...)`, never to a
# panic (a panicking funnel is a dlclose hazard and unwinds across the C ABI).
ENGINE_CONTROL = Path("hegel-c/src/control.rs")
PANIC_MACRO = re.compile(r"(?<![\w!])panic!\s*\(")


def main() -> int:
    # The engine lives in hegel-c/src (and the frontend in src/); the
    # internal-assert discipline applies to both so a panic can't escape a
    # generator or cross the FFI boundary uncontrolled.
    roots = [Path("src"), Path("hegel-c/src")]
    offences: list[str] = []
    for root in roots:
        for path in sorted(root.rglob("*.rs")):
            for lineno, line in enumerate(path.read_text().splitlines(), start=1):
                if line.lstrip().startswith("//"):
                    continue
                if ASSERT_MACRO.search(line):
                    offences.append(f"  {path}:{lineno}: {line.strip()}")

    panic_offences: list[str] = []
    for lineno, line in enumerate(ENGINE_CONTROL.read_text().splitlines(), start=1):
        if line.lstrip().startswith("//"):
            continue
        if PANIC_MACRO.search(line):
            panic_offences.append(f"  {ENGINE_CONTROL}:{lineno}: {line.strip()}")

    if offences:
        print("std assertion macros are not allowed in src/ or hegel-c/src/.")
        print("Use hegel_internal_assert! (internal invariants) or")
        print("invalid_argument! / EngineError::InvalidArgument (user-facing")
        print("argument validation) instead:")
        print()
        print("\n".join(offences))

    if panic_offences:
        print("hegel-c's internal-error funnel must return Err, not panic.")
        print("hegel_internal_assert! and friends expand to")
        print("`return Err(InternalError::new(...).into())`; do not")
        print("reintroduce a panic! into hegel-c/src/control.rs:")
        print()
        print("\n".join(panic_offences))

    if offences or panic_offences:
        return 1

    print("check-internal-asserts: OK")
    return 0


if __name__ == "__main__":
    sys.exit(main())
