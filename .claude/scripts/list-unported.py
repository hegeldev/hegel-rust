#!/usr/bin/env python3
"""List upstream test files that have not yet been ported.

Usage:
    list-unported.py --kind pbtkit [--smallest N]
    list-unported.py --kind hypothesis [--smallest N]

Matches upstream `test_<name>.py` files against local `tests/<kind>/<name>.rs`
files, filters out anything listed in SKIPPED.md, and prints the remaining
upstream paths one per line.

With --smallest N, sorts remaining files by line count (ascending) and
prints only the first N. Used by the Stop hook's porting gates to pick
one file per iteration.
"""

from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path


PBTKIT_DIR = Path("/tmp/pbtkit/tests")
HYPOTHESIS_DIR = Path("/tmp/hypothesis/hypothesis-python/tests/cover")


def read_skipped(kind: str) -> set[str]:
    """Parse SKIPPED.md and return the set of skipped filenames for `kind`.

    We split the file on its ``## pbtkit`` / ``## hypothesis`` headers
    (case-insensitive) and capture ``test_*.py`` filenames in bullets.
    """
    skipped_md = Path("SKIPPED.md")
    if not skipped_md.exists():
        return set()
    text = skipped_md.read_text()

    sections: dict[str, str] = {}
    current: str | None = None
    buf: list[str] = []
    for line in text.splitlines():
        m = re.match(r"^\s*##\s+(\w+)", line)
        if m:
            if current is not None:
                sections[current] = "\n".join(buf)
            current = m.group(1).lower()
            buf = []
        elif current is not None:
            buf.append(line)
    if current is not None:
        sections[current] = "\n".join(buf)

    body = sections.get(kind.lower(), "")
    return set(re.findall(r"`(test_[\w_]+\.py)`", body))


def upstream_dir_for(kind: str) -> Path:
    if kind == "pbtkit":
        return PBTKIT_DIR
    if kind == "hypothesis":
        return HYPOTHESIS_DIR
    raise SystemExit(f"unknown kind: {kind}")


def ported_stems(kind: str) -> set[str]:
    """Return the set of module stems that already correspond to an upstream file.

    We look in the natural home (``tests/<kind>/``) AND in a small allowlist
    of other hegel-rust test directories that predate this harness but still
    count as "ported". That way an upstream ``test_composite.py`` in
    ``shrink_quality/`` is recognised as ported by
    ``tests/test_shrink_quality/composite.rs`` instead of being relisted
    as unported work.
    """
    search_dirs = [Path(f"tests/{kind}")]
    if kind == "pbtkit":
        search_dirs += [
            Path("tests/test_shrink_quality"),
            Path("tests/test_find_quality"),
            Path("tests/embedded/native"),
        ]
    stems: set[str] = set()
    for d in search_dirs:
        if not d.exists():
            continue
        for p in d.rglob("*.rs"):
            if p.name == "main.rs":
                continue
            stems.add(p.stem)
            # Existing embedded tests like `choices_tests.rs` cover the
            # choice types; treat the `_tests` suffix as equivalent to
            # the bare stem so upstream `test_choice.py` matches.
            if p.stem.endswith("_tests"):
                stems.add(p.stem[: -len("_tests")])
    return stems


def upstream_files(kind: str) -> list[Path]:
    root = upstream_dir_for(kind)
    if not root.exists():
        return []
    # pbtkit has nested subdirs (findability/, shrink_quality/).
    return sorted(root.rglob("test_*.py"))


def unported(kind: str) -> list[Path]:
    skipped = read_skipped(kind)
    ported = ported_stems(kind)
    result = []
    for path in upstream_files(kind):
        if path.name in skipped:
            continue
        # Rust module name drops the ``test_`` prefix.
        stem = path.stem.removeprefix("test_") if path.stem.startswith("test_") else path.stem
        if stem in ported:
            continue
        result.append(path)
    return result


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--kind", required=True, choices=["pbtkit", "hypothesis"])
    parser.add_argument(
        "--smallest",
        type=int,
        default=0,
        help="If N>0, sort by line count ascending and print only the first N.",
    )
    args = parser.parse_args()

    files = unported(args.kind)
    if args.smallest > 0:
        files.sort(key=lambda p: (sum(1 for _ in p.open()), p.name))
        files = files[: args.smallest]
    else:
        files.sort()

    for f in files:
        print(f)
    return 0


if __name__ == "__main__":
    sys.exit(main())
