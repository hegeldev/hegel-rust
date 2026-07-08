#!/usr/bin/env python3
"""Check that test code calls generators through the `gs` alias.

The testing convention is to import generators as
`use hegel::generators as gs;` (`use crate::generators as gs;` in
embedded tests) and call them as `gs::integers()`, `gs::booleans()`,
etc. — including inside string-literal code snippets such as
`TempRustProject` sources. This script flags qualified call paths like
`hegel::generators::integers()` and item imports of generator functions
like `use crate::generators::{booleans, integers};`.

Importing traits and types directly (`Generator`, `DefaultGenerator`,
`BoxedGenerator`) is allowed, as is the deliberate glob-import test in
test_combinators.rs (a glob is not a qualified path, so it does not
match).
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

TESTS_ROOT = Path("tests")

QUALIFIED_RE = re.compile(r"\b(?:hegel|crate)::generators::(?!self\b)[a-z]\w*")
BRACE_IMPORT_RE = re.compile(r"use\s+(?:hegel|crate)::generators::\{([^}]*)\}")


def brace_import_violates(items: str) -> bool:
    for item in items.split(","):
        name = item.strip().split()[0] if item.strip() else ""
        if name and name != "self" and name[0].islower():
            return True
    return False


def check() -> int:
    violations: list[str] = []

    for rs_file in sorted(TESTS_ROOT.rglob("*.rs")):
        text = rs_file.read_text()
        # Join rustfmt-wrapped imports so a multi-line
        # `use hegel::generators::{\n    booleans,\n};` still matches the
        # single-line patterns; record the original line number of the first
        # physical line of each joined statement.
        lines = text.splitlines()
        logical: list[tuple[int, str]] = []
        i = 0
        while i < len(lines):
            line = lines[i]
            start = i
            if re.match(r"\s*use\b", line):
                while "{" in line and "}" not in line and i + 1 < len(lines):
                    i += 1
                    line = line + " " + lines[i].strip()
            logical.append((start + 1, line))
            i += 1
        for lineno, line in logical:
            brace = BRACE_IMPORT_RE.search(line)
            if brace and brace_import_violates(brace.group(1)):
                violations.append(f"  {rs_file}:{lineno}: {line.strip()}")
            elif not brace and QUALIFIED_RE.search(line):
                violations.append(f"  {rs_file}:{lineno}: {line.strip()}")

    if violations:
        print("Generator calls not using the `gs` alias:\n")
        for v in violations:
            print(v)
        print(
            "\nImport generators as `use hegel::generators as gs;` (or"
            " `use crate::generators as gs;` in embedded tests) and call"
            " them as `gs::<name>`. This applies inside string-literal"
            " code snippets too."
        )
        return 1

    return 0


if __name__ == "__main__":
    sys.exit(check())
