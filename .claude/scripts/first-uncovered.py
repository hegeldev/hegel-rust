#!/usr/bin/env python3
"""Print the first uncovered line in src/native/ per `scripts/check-coverage.py --native`.

Runs the existing coverage script in native mode, parses its "Found uncovered
CODE that requires tests" output, and emits the first file:line plus 5 lines
of surrounding context. Used by the Stop hook's native-coverage gate to pick
exactly one line per iteration.

Exits 0 if native coverage is clean (no output). Exits 1 and prints the
target line if a gap exists.
"""

from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path


def main() -> int:
    proc = subprocess.run(
        ["scripts/check-coverage.py", "--native"],
        capture_output=True,
        text=True,
        timeout=600,
    )
    out = proc.stdout + "\n" + proc.stderr

    # Lines of the form:   src/native/runner.rs:83: content
    # The leading indentation comes from check-coverage.py's reporter.
    pat = re.compile(r"^\s+(src/native/[^\s:]+):(\d+):\s*(.*)$", re.MULTILINE)
    m = pat.search(out)
    if m is None:
        # Clean — nothing to report.
        return 0

    path = Path(m.group(1))
    line = int(m.group(2))
    content = m.group(3)

    print(f"{path}:{line}: {content}")
    print()
    print("Context:")
    try:
        lines = path.read_text().splitlines()
    except OSError:
        return 1
    start = max(0, line - 4)
    end = min(len(lines), line + 3)
    for i in range(start, end):
        marker = ">>" if (i + 1) == line else "  "
        print(f"  {marker} {i + 1:5}: {lines[i]}")
    return 1


if __name__ == "__main__":
    sys.exit(main())
