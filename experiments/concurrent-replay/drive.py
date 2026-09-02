#!/usr/bin/env python3
import re
import shutil
import subprocess
import sys
import tempfile
import time
from pathlib import Path

BIN = Path(__file__).parent / "target/release/exp007"
BLOB_RE = re.compile(r'reproduce_failure\("([^"]+)"\)')

def run(args):
    t0 = time.time()
    p = subprocess.run([str(BIN), *args], capture_output=True, text=True, timeout=300)
    out = p.stdout + p.stderr
    return out, time.time() - t0

def classify(out):
    if "no longer reproduces" in out:
        return "stale"
    if "EXP007-RESULT: FAILED" in out:
        return "failed"
    return "passed"

def campaign(kind, trials, replays_per_trial):
    stats = {
        "discover_found": 0, "discover_missed": 0, "discover_secs": [],
        "blob_present": 0, "caveats": {},
        "reuse_reproduced": 0, "reuse_missed": 0, "reuse_secs": [],
        "reuse_caveats": {},
        "replay_real": 0, "replay_stale": 0,
        "clone_values": [],
    }
    for i in range(trials):
        db = tempfile.mkdtemp(prefix=f"exp007-{kind}-")
        try:
            out, secs = run([f"discover-{kind}", db])
            if classify(out) != "failed":
                stats["discover_missed"] += 1
                continue
            stats["discover_found"] += 1
            stats["discover_secs"].append(secs)
            for line in out.splitlines():
                if line.startswith("note: "):
                    key = line[6:].split(":")[0]
                    stats["caveats"][key] = stats["caveats"].get(key, 0) + 1
            m = BLOB_RE.search(out)
            blob = m.group(1) if m else None
            if blob:
                stats["blob_present"] += 1

            out2, secs2 = run([f"reuse-{kind}", db])
            if classify(out2) == "failed":
                stats["reuse_reproduced"] += 1
                stats["reuse_secs"].append(secs2)
                for line in out2.splitlines():
                    if line.startswith("note: "):
                        key = line[6:].split(":")[0]
                        stats["reuse_caveats"][key] = stats["reuse_caveats"].get(key, 0) + 1
            else:
                stats["reuse_missed"] += 1

            if blob:
                for _ in range(replays_per_trial):
                    out3, _ = run([f"replay-{kind}", blob])
                    verdict = classify(out3)
                    if verdict == "failed":
                        stats["replay_real"] += 1
                        vm = re.search(r"clone-flaky: x = (\d+)", out3)
                        if vm:
                            stats["clone_values"].append(int(vm.group(1)))
                    elif verdict == "stale":
                        stats["replay_stale"] += 1
        finally:
            shutil.rmtree(db, ignore_errors=True)
    return stats

def report(kind, s, trials):
    print(f"== {kind} ({trials} trials) ==")
    print(f"discovery: {s['discover_found']}/{trials} found the bug"
          + (f", median {sorted(s['discover_secs'])[len(s['discover_secs'])//2]:.2f}s" if s['discover_secs'] else ""))
    print(f"blob printed: {s['blob_present']}/{s['discover_found']}")
    print(f"discovery caveats: {s['caveats']}")
    print(f"reuse: {s['reuse_reproduced']} reproduced, {s['reuse_missed']} missed"
          + (f", median {sorted(s['reuse_secs'])[len(s['reuse_secs'])//2]:.2f}s" if s['reuse_secs'] else ""))
    print(f"reuse caveats: {s['reuse_caveats']}")
    total_replays = s['replay_real'] + s['replay_stale']
    print(f"blob replay: {s['replay_real']}/{total_replays} reproduced")
    if s['clone_values']:
        print(f"clone replay values: {sorted(set(s['clone_values']))}")
    print()

if __name__ == "__main__":
    trials = int(sys.argv[1]) if len(sys.argv) > 1 else 20
    for kind in ["racy", "clone"]:
        report(kind, campaign(kind, trials, 3), trials)
