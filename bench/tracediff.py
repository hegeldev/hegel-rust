#!/usr/bin/env python3
"""Check that two hegel-bench builds behave identically.

Runs every workload under `--trace` (debug verbosity: every drawn value,
every case's status, the shrink phases) with both binaries and reports any
workload whose traces differ. Composite generation workloads are also run at
1000 test cases so span mutation gets a longer run.

Usage: tracediff.py OLD_BINARY [NEW_BINARY]
"""
import subprocess
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
COMPOSITES = [
    "vec_i32_100",
    "three_versions",
    "recursive_tree",
    "machine_map",
    "hashmap_i32_50",
    "vec_bool_1000",
]


def trace(binary, workload, cases):
    args = [binary, workload, "--trace"]
    if cases:
        args += ["--test-cases", str(cases)]
    out = subprocess.run(args, capture_output=True, text=True).stderr
    return "\n".join(l for l in out.splitlines() if "elapsed_us" not in l)


def main():
    old = sys.argv[1]
    new = sys.argv[2] if len(sys.argv) > 2 else str(HERE / "target/release/hegel-bench")
    listing = subprocess.run([new, "--list"], capture_output=True, text=True, check=True)
    workloads = [line.split()[0] for line in listing.stdout.splitlines() if line.strip()]
    runs = [(w, None) for w in workloads] + [(w, 1000) for w in COMPOSITES]
    differing = 0
    for workload, cases in runs:
        a, b = trace(old, workload, cases), trace(new, workload, cases)
        label = workload if cases is None else f"{workload}@{cases}"
        if a == b:
            print(f"same     {label} ({len(a.splitlines())} lines)")
        else:
            differing += 1
            print(f"DIFFERS  {label}")
    print(f"{len(runs)} runs, {differing} differ")
    sys.exit(1 if differing else 0)


if __name__ == "__main__":
    main()
