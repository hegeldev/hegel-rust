#!/usr/bin/env python3
"""Check that every .rs file in `tests/<lib>/` is wired into `<lib>/main.rs`.

Cargo's integration-test discovery only looks at `tests/*.rs` and
`tests/*/main.rs`. Sibling files like `tests/jiff/civil.rs` are only
compiled if `tests/jiff/main.rs` declares them via `mod civil;`. Without
that declaration, the file is silently skipped — no warning, no error,
no failing test. This script catches such orphans.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

TESTS_ROOT = Path("tests")
MOD_RE = re.compile(r"^\s*(?:pub\s+)?mod\s+(\w+)\s*;", re.MULTILINE)


def check() -> int:
    violations: list[str] = []

    for lib_dir in sorted(TESTS_ROOT.iterdir()):
        if not lib_dir.is_dir():
            continue
        main_rs = lib_dir / "main.rs"
        if not main_rs.exists():
            continue

        declared = set(MOD_RE.findall(main_rs.read_text()))

        for rs_file in sorted(lib_dir.iterdir()):
            if not rs_file.is_file() or rs_file.suffix != ".rs":
                continue
            if rs_file.name == "main.rs":
                continue
            if rs_file.stem not in declared:
                violations.append(
                    f"  {rs_file}: not referenced from {main_rs} "
                    f"(add `mod {rs_file.stem};` to {main_rs})"
                )

    if violations:
        print("Orphan test files found:\n")
        for v in violations:
            print(v)
        print(
            "\nFiles in tests/<lib>/ are only compiled if main.rs declares them"
            " with `mod <name>;`. Add the declaration, or delete the file."
        )
        return 1

    return 0


if __name__ == "__main__":
    sys.exit(check())
