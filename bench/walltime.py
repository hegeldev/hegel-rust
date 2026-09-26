#!/usr/bin/env python3
"""Interleaved walltime comparison of two hegel-bench builds.

    walltime.py OLD_BINARY NEW_BINARY [--rounds R] [-n N] [--test-cases C] [--out FILE] [WORKLOAD...]

Runs each workload N times per binary per round, alternating the binaries between rounds so
that slow drift of the machine hits both alike, and reports the minimum and median of the
R*N samples per binary with their relative change. Without workloads, every workload the
new binary lists is measured. The samples are saved as JSON when --out is given.
"""

import argparse
import json
import re
import subprocess


def run_once(binary, workload, test_cases):
    proc = subprocess.run(
        [binary, workload, "--test-cases", str(test_cases)],
        check=True,
        capture_output=True,
        text=True,
    )
    for line in proc.stdout.splitlines():
        m = re.match(r"hegel-bench workload=(\S+) repeat=(\d+) cases=(\d+) elapsed_us=(\d+)", line)
        if m:
            return int(m.group(4))
    raise SystemExit(f"no result line in output of {binary} {workload}:\n{proc.stdout}")


def workloads_of(binary):
    out = subprocess.run([binary, "--list"], check=True, capture_output=True, text=True).stdout
    return [line.split()[0] for line in out.splitlines() if line.strip()]


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("old")
    parser.add_argument("new")
    parser.add_argument("--rounds", type=int, default=2)
    parser.add_argument("-n", type=int, default=5)
    parser.add_argument("--test-cases", type=int, default=1000)
    parser.add_argument("--out")
    parser.add_argument("workloads", nargs="*")
    args = parser.parse_args()
    names = args.workloads or workloads_of(args.new)
    results = {name: {"old": [], "new": []} for name in names}
    for round_index in range(args.rounds):
        order = [("old", args.old), ("new", args.new)]
        if round_index % 2:
            order.reverse()
        for name in names:
            for side, binary in order:
                for _ in range(args.n):
                    results[name][side].append(run_once(binary, name, args.test_cases))
    print(f"{'workload':32} {'old min':>10} {'new min':>10} {'change':>8} {'old med':>10} {'new med':>10} {'change':>8}")
    for name in names:
        old = sorted(results[name]["old"])
        new = sorted(results[name]["new"])
        old_min, new_min = old[0], new[0]
        old_med, new_med = old[len(old) // 2], new[len(new) // 2]
        print(
            f"{name:32} {old_min:>10,} {new_min:>10,} {100 * (new_min - old_min) / old_min:>+7.1f}%"
            f" {old_med:>10,} {new_med:>10,} {100 * (new_med - old_med) / old_med:>+7.1f}%"
        )
    if args.out:
        with open(args.out, "w") as f:
            json.dump(
                {
                    "old": args.old,
                    "new": args.new,
                    "test_cases": args.test_cases,
                    "rounds": args.rounds,
                    "n": args.n,
                    "results": results,
                },
                f,
                indent=2,
            )
            f.write("\n")
        print(f"wrote {args.out}")


if __name__ == "__main__":
    main()
