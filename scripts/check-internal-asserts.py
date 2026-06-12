#!/usr/bin/env python3
"""Forbid std assertion macros in src/.

A plain `assert!` (or `assert_eq!` / `assert_ne!` / `debug_assert*!`) that
fires inside a running test body unwinds exactly like a failing property:
the engine classifies it as a counterexample, spends the post-bug window
and the shrink budget "minimizing" a framework bug, and reports it with a
reproducer blob.

Instead:

- Internal invariants (bugs in hegel itself) must use the
  `hegel_internal_assert!` family from `src/control.rs`, which aborts the
  run immediately with a bug-report message.
- User-facing argument validation must use `invalid_argument!` from
  `src/test_case.rs`, which aborts the run with the usage error.

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


def main() -> int:
    offences: list[str] = []
    for path in sorted(Path("src").rglob("*.rs")):
        for lineno, line in enumerate(path.read_text().splitlines(), start=1):
            if line.lstrip().startswith("//"):
                continue
            if ASSERT_MACRO.search(line):
                offences.append(f"  {path}:{lineno}: {line.strip()}")

    if offences:
        print("std assertion macros are not allowed in src/.")
        print("Use hegel_internal_assert! (internal invariants) or")
        print("invalid_argument! (user-facing argument validation) instead:")
        print()
        print("\n".join(offences))
        return 1

    print("check-internal-asserts: OK")
    return 0


if __name__ == "__main__":
    sys.exit(main())
