#!/usr/bin/env python3
"""Generate src/unicodedata/{categories,nfd_bases}.txt from UnicodeData.txt.

Run from the repo root:

    python scripts/generate_unicodedata_tables.py

Reads `src/unicodedata/UnicodeData.txt` (vendored from
https://www.unicode.org/Public/15.1.0/ucd/UnicodeData.txt) and writes two
compact text files included into the Rust crate via `include_str!`:

  - `categories.txt`: one line per contiguous run, `<hex_end> <cat>`,
    where `cat` is a two-character General Category code. Entries are
    non-overlapping and sorted by `end`; together they tile `0..=0x10FFFF`.
    Codepoints not listed in UnicodeData.txt are reported as `Cn`, matching
    Python's `unicodedata.category`.

  - `nfd_bases.txt`: one line per canonically-decomposable codepoint,
    `<hex_cp> <hex_base>`, sorted by `cp`. The base is the recursive NFD
    base (chain followed to its fixed point).

Both files are parsed lazily into `OnceLock`s at first lookup.
"""

from __future__ import annotations

import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
UCD_PATH = REPO_ROOT / "src" / "unicodedata" / "UnicodeData.txt"
CATEGORIES_PATH = REPO_ROOT / "src" / "unicodedata" / "categories.txt"
NFD_BASES_PATH = REPO_ROOT / "src" / "unicodedata" / "nfd_bases.txt"

# Categories in the same order as the Rust enum. Keeping them in a stable
# order means the generated file only churns when the underlying data
# changes.
CATEGORIES = [
    "Lu", "Ll", "Lt", "Lm", "Lo",
    "Mn", "Mc", "Me",
    "Nd", "Nl", "No",
    "Pc", "Pd", "Ps", "Pe", "Pi", "Pf", "Po",
    "Sm", "Sc", "Sk", "So",
    "Zs", "Zl", "Zp",
    "Cc", "Cf", "Cs", "Co", "Cn",
]

MAX_CP = 0x10FFFF


def parse_unicode_data(path: Path) -> dict[int, str]:
    """Parse UnicodeData.txt into {codepoint: category}.

    Expands the "<..., First>" / "<..., Last>" range markers.
    """
    entries: dict[int, str] = {}
    lines = [l for l in path.read_text().splitlines() if l]
    i = 0
    while i < len(lines):
        fields = lines[i].split(";")
        cp = int(fields[0], 16)
        name = fields[1]
        cat = fields[2]
        if name.endswith(", First>"):
            i += 1
            fields2 = lines[i].split(";")
            cp_end = int(fields2[0], 16)
            assert fields2[1].endswith(", Last>"), fields2
            for c in range(cp, cp_end + 1):
                entries[c] = cat
        else:
            entries[cp] = cat
        i += 1
    return entries


def parse_canonical_decompositions(path: Path) -> dict[int, int]:
    """Parse the recursive NFD base codepoint for each canonically-decomposable codepoint.

    Field 5 of UnicodeData.txt holds the decomposition mapping. Entries
    starting with `<...>` are *compatibility* decompositions (used by NFKD,
    not NFD); we ignore those. Canonical decompositions are space-separated
    hex codepoints; the first is the "base" and the rest are combining marks.

    The base may itself decompose (e.g. Ǻ → Å + combining-acute, and Å
    decomposes further to A + combining-ring). We follow the chain to
    its fixed point so the final mapping always points at a non-decomposable
    starting codepoint.
    """
    immediate: dict[int, int] = {}
    for line in path.read_text().splitlines():
        if not line:
            continue
        fields = line.split(";")
        cp = int(fields[0], 16)
        decomp = fields[5]
        if not decomp or decomp.startswith("<"):
            continue
        first = int(decomp.split()[0], 16)
        if first != cp:
            immediate[cp] = first

    # Resolve recursively. Cycle detection is defensive — Unicode canonical
    # decompositions are guaranteed acyclic, but we don't rely on that.
    recursive: dict[int, int] = {}
    for cp in immediate:
        seen: set[int] = set()
        current = cp
        while current in immediate and current not in seen:
            seen.add(current)
            current = immediate[current]
        if current != cp:
            recursive[cp] = current
    return recursive


def build_ranges(entries: dict[int, str]) -> list[tuple[int, int, str]]:
    """Collapse per-codepoint categories into contiguous runs.

    Codepoints not in `entries` default to `Cn`.
    """
    ranges: list[tuple[int, int, str]] = []
    current_cat: str | None = None
    current_start = 0
    for cp in range(MAX_CP + 1):
        cat = entries.get(cp, "Cn")
        if cat != current_cat:
            if current_cat is not None:
                ranges.append((current_start, cp - 1, current_cat))
            current_cat = cat
            current_start = cp
    assert current_cat is not None
    ranges.append((current_start, MAX_CP, current_cat))
    return ranges


def emit_categories(ranges: list[tuple[int, int, str]], path: Path) -> None:
    seen_cats = sorted({r[2] for r in ranges})
    unknown = [c for c in seen_cats if c not in CATEGORIES]
    if unknown:
        sys.exit(f"Unknown categories in UnicodeData.txt: {unknown}")
    lines = [f"{end:x} {cat}" for _, end, cat in ranges]
    path.write_text("\n".join(lines) + "\n")


def emit_nfd_bases(nfd_bases: dict[int, int], path: Path) -> None:
    lines = [f"{cp:x} {nfd_bases[cp]:x}" for cp in sorted(nfd_bases)]
    path.write_text("\n".join(lines) + "\n")


def main() -> None:
    entries = parse_unicode_data(UCD_PATH)
    ranges = build_ranges(entries)
    nfd_bases = parse_canonical_decompositions(UCD_PATH)
    emit_categories(ranges, CATEGORIES_PATH)
    emit_nfd_bases(nfd_bases, NFD_BASES_PATH)
    print(
        f"Wrote {CATEGORIES_PATH} ({len(ranges)} ranges) and "
        f"{NFD_BASES_PATH} ({len(nfd_bases)} NFD base entries)."
    )


if __name__ == "__main__":
    main()
