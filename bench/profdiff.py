#!/usr/bin/env python3
"""Diff two callgrind profiles function by function.

    profdiff.py OLD_LABEL NEW_LABEL WORKLOAD [-n N] [--exclusive]

Reads results/callgrind/<label>.<workload>.out for both labels, annotates
each (inclusive by default), strips source paths, joins by function name and
prints the N functions whose instruction counts moved most, largest first.
"""

from __future__ import annotations

import argparse
import re
import subprocess
from pathlib import Path

HERE = Path(__file__).resolve().parent
PATH_PREFIX = re.compile(r"^(?:/[^:\s]+/)?([^/:\s]+):")


def annotate(label: str, workload: str, inclusive: bool) -> dict[str, int]:
    out = HERE / "results" / "callgrind" / f"{label}.{workload}.out"
    args = ["callgrind_annotate", "--threshold=100"]
    if inclusive:
        args.append("--inclusive=yes")
    text = subprocess.run(
        args + [str(out)], check=True, capture_output=True, text=True
    ).stdout
    counts: dict[str, int] = {}
    for line in text.splitlines():
        m = re.match(r"^\s*([\d,]+)\s+\([^)]*\)\s+(\S.*)$", line)
        if not m:
            continue
        name = m.group(2).split(" [")[0]
        name = PATH_PREFIX.sub(r"\1:", name)
        counts[name] = counts.get(name, 0) + int(m.group(1).replace(",", ""))
    return counts


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("old")
    ap.add_argument("new")
    ap.add_argument("workload")
    ap.add_argument("-n", type=int, default=30)
    ap.add_argument("--exclusive", action="store_true")
    a = ap.parse_args()
    old = annotate(a.old, a.workload, not a.exclusive)
    new = annotate(a.new, a.workload, not a.exclusive)
    rows = [(new.get(k, 0) - old.get(k, 0), old.get(k, 0), new.get(k, 0), k) for k in set(old) | set(new)]
    rows.sort(key=lambda r: -abs(r[0]))
    print(f"{'delta':>12} {'old':>12} {'new':>12}  function")
    for delta, o, n, k in rows[: a.n]:
        print(f"{delta:>+12,} {o:>12,} {n:>12,}  {k[:150]}")


if __name__ == "__main__":
    main()
