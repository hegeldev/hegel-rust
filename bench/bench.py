#!/usr/bin/env python3
"""Deterministic and walltime benchmarks of hegel-rust.

    bench.py build                          build the release hegel-bench binary
    bench.py list                           list workloads
    bench.py run LABEL [WORKLOAD...]        callgrind every workload, save results/LABEL.json
    bench.py time LABEL [-n N] [WORKLOAD...]  walltime (min of N runs), save results/LABEL.time.json
    bench.py compare OLD NEW                table of instruction-count deltas between two labels
    bench.py annotate LABEL WORKLOAD [-n N] top N functions by exclusive instructions

Instruction counts come from valgrind's callgrind, restricted to the `measured` function of the
benchmark binary, so process startup and argument parsing are excluded. They are deterministic for
a given binary, seed and test-case budget, which makes them the inner loop for optimisation;
walltime is for validating that a change in instructions is a change in time.
"""

import argparse
import json
import os
import re
import shutil
import subprocess
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
TARGET = Path(os.environ.get("CARGO_TARGET_DIR", HERE / "target"))
BINARY = TARGET / "release" / "hegel-bench"
RESULTS = HERE / "results"
CALLGRIND_OUT = RESULTS / "callgrind"


def build():
    subprocess.run(["cargo", "build", "--release"], cwd=HERE, check=True)


def workloads():
    out = subprocess.run([BINARY, "--list"], check=True, capture_output=True, text=True).stdout
    return [line.split()[0] for line in out.splitlines() if line.strip()]


def parse_output(stdout):
    cases = elapsed = None
    for line in stdout.splitlines():
        m = re.match(r"hegel-bench workload=(\S+) repeat=(\d+) cases=(\d+) elapsed_us=(\d+)", line)
        if m:
            cases = int(m.group(3))
            elapsed = int(m.group(4))
    if cases is None:
        raise SystemExit(f"no result line in output:\n{stdout}")
    return cases, elapsed


def callgrind_total(path):
    for line in path.read_text().splitlines():
        if line.startswith("summary:") or line.startswith("totals:"):
            return int(line.split()[1])
    raise SystemExit(f"no summary in {path}")


def run(label, names, test_cases, binary=BINARY):
    CALLGRIND_OUT.mkdir(parents=True, exist_ok=True)
    results = {}
    for name in names:
        out_file = CALLGRIND_OUT / f"{label}.{name}.out"
        cmd = [
            "valgrind",
            "--tool=callgrind",
            "--quiet",
            f"--callgrind-out-file={out_file}",
            "--toggle-collect=hegel_bench::measured*",
            str(binary),
            name,
            "--test-cases",
            str(test_cases),
        ]
        proc = subprocess.run(cmd, capture_output=True, text=True)
        if proc.returncode != 0:
            sys.stderr.write(proc.stderr)
            raise SystemExit(f"{name}: exit {proc.returncode}")
        cases, _ = parse_output(proc.stdout)
        ir = callgrind_total(out_file)
        results[name] = {"ir": ir, "cases": cases, "ir_per_case": ir / cases if cases else None}
        print(f"{name:32} {ir:>16,} Ir  {cases:>6} cases  {ir / max(cases, 1):>14,.0f} Ir/case")
    path = RESULTS / f"{label}.json"
    path.write_text(json.dumps({"test_cases": test_cases, "workloads": results}, indent=2) + "\n")
    print(f"wrote {path}")


def time(label, names, test_cases, n):
    RESULTS.mkdir(parents=True, exist_ok=True)
    results = {}
    for name in names:
        samples = []
        for _ in range(n):
            proc = subprocess.run(
                [BINARY, name, "--test-cases", str(test_cases)],
                check=True,
                capture_output=True,
                text=True,
            )
            cases, elapsed = parse_output(proc.stdout)
            samples.append(elapsed)
        samples.sort()
        best, median = samples[0], samples[len(samples) // 2]
        results[name] = {"min_us": best, "median_us": median, "cases": cases, "samples": samples}
        print(f"{name:32} min {best:>10,} us  median {median:>10,} us  {cases:>6} cases")
    path = RESULTS / f"{label}.time.json"
    path.write_text(json.dumps({"test_cases": test_cases, "workloads": results}, indent=2) + "\n")
    print(f"wrote {path}")


def compare(old, new):
    a = json.loads((RESULTS / f"{old}.json").read_text())["workloads"]
    b = json.loads((RESULTS / f"{new}.json").read_text())["workloads"]
    print(f"{'workload':32} {old:>16} {new:>16} {'change':>9}")
    for name in sorted(set(a) | set(b)):
        if name not in a or name not in b:
            print(f"{name:32} {'-' if name not in a else a[name]['ir']:>16} {'-' if name not in b else b[name]['ir']:>16}")
            continue
        x, y = a[name]["ir"], b[name]["ir"]
        print(f"{name:32} {x:>16,} {y:>16,} {100 * (y - x) / x:>+8.2f}%")


def annotate(label, name, n):
    out_file = CALLGRIND_OUT / f"{label}.{name}.out"
    proc = subprocess.run(
        ["callgrind_annotate", "--tree=none", "--show-percs=yes", str(out_file)],
        check=True,
        capture_output=True,
        text=True,
    )
    lines = proc.stdout.splitlines()
    start = next(i for i, line in enumerate(lines) if "file:function" in line)
    for line in lines[start : start + n + 2]:
        print(line)


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = parser.add_subparsers(dest="command", required=True)
    sub.add_parser("build")
    sub.add_parser("list")
    p = sub.add_parser("run")
    p.add_argument("label")
    p.add_argument("workloads", nargs="*")
    p.add_argument("--test-cases", type=int, default=100)
    p.add_argument("--binary", type=Path, default=BINARY,
                   help="measure this binary instead of the built one (from this directory: "
                        "profile resolution walks the working directory's parents for hegel.toml, "
                        "so a run's fixed cost depends on the cwd)")
    p = sub.add_parser("time")
    p.add_argument("label")
    p.add_argument("workloads", nargs="*")
    p.add_argument("--test-cases", type=int, default=100)
    p.add_argument("-n", type=int, default=10)
    p = sub.add_parser("compare")
    p.add_argument("old")
    p.add_argument("new")
    p = sub.add_parser("annotate")
    p.add_argument("label")
    p.add_argument("workload")
    p.add_argument("-n", type=int, default=30)
    args = parser.parse_args()

    if args.command == "build":
        build()
    elif args.command == "list":
        print("\n".join(workloads()))
    elif args.command == "run":
        if shutil.which("valgrind") is None:
            raise SystemExit("valgrind not found: run this on the Linux VM")
        run(args.label, args.workloads or workloads(), args.test_cases, args.binary)
    elif args.command == "time":
        time(args.label, args.workloads or workloads(), args.test_cases, args.n)
    elif args.command == "compare":
        compare(args.old, args.new)
    elif args.command == "annotate":
        annotate(args.label, args.workload, args.n)


if __name__ == "__main__":
    main()
