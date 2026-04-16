#!/usr/bin/env python3
"""Count remaining CANARY panics in src/native/.

Each `panic!("CANARY:...")` line marks a code path believed to be
unreachable. When a test reaches one, the panic fires and the fix is to
delete the canary (restoring the real code below). This script reports how
many remain.

Usage:
    count-canaries.py           # print total count
    count-canaries.py --list    # print each canary location
"""

from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path


CANARY_RE = re.compile(r'panic!\("CANARY:([^"]+)"')


def scan() -> list[tuple[Path, int, str]]:
    root = Path("src/native")
    if not root.exists():
        return []
    hits: list[tuple[Path, int, str]] = []
    for path in sorted(root.rglob("*.rs")):
        try:
            for i, line in enumerate(path.open(), 1):
                m = CANARY_RE.search(line)
                if m:
                    hits.append((path, i, m.group(1)))
        except OSError:
            continue
    return hits


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--list", action="store_true")
    args = parser.parse_args()

    hits = scan()
    if args.list:
        for path, line, marker in hits:
            print(f"{path}:{line}: CANARY:{marker}")
    else:
        print(len(hits))
    return 0


if __name__ == "__main__":
    sys.exit(main())
