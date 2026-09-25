#!/usr/bin/env python3
"""Print the caller blocks of every function whose name matches a pattern.

Usage: callers.py LABEL WORKLOAD PATTERN [PATTERN...]
"""
import re
import subprocess
import sys

label, workload, *patterns = sys.argv[1:]
out = subprocess.run(
    ["callgrind_annotate", "--tree=caller", "--show-percs=yes",
     f"results/callgrind/{label}.{workload}.out"],
    capture_output=True, text=True, check=True).stdout
clean = lambda s: re.sub(r"\[/home[^\]]*\]", "", re.sub(
    r"/home/exedev/[^ ]*/hegel-c/src/|/rustc/[^ ]*/library/|/home/exedev/.cargo/registry/src/[^/]*/", "", s))
for block in out.split("\n\n"):
    star = [l for l in block.splitlines() if re.match(r"\s*[\d,]+ \([ \d.]+%\)\s+\*", l)]
    if star and any(p in star[0] for p in patterns):
        lines = [clean(l)[:170] for l in block.splitlines()]
        callers = sorted((l for l in lines if " < " in l), key=lambda l: -int(l.split()[0].replace(",", "")))
        print(clean(star[0])[:170])
        for l in callers[:12]:
            print("   ", l.strip())
        print()
