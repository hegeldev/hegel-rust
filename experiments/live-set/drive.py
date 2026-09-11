#!/usr/bin/env python3
"""Drive the live-set experiment: build the harness, run episodes per body,
write per-episode JSON lines, and print a markdown summary per body."""
import argparse
import json
import os
import re
import shutil
import statistics
import subprocess
import sys
import tempfile
import time
from collections import Counter
from pathlib import Path

HERE = Path(__file__).resolve().parent
TARGET = Path(os.environ.get("CARGO_TARGET_DIR", "/tmp/hegel-exp-target"))
BIN = TARGET / "release" / "livesets"
BLOB_RE = re.compile(r'reproduce_failure\("([^"]+)"\)')
BODIES = ["racy", "clone", "branch", "twobranch"]
TIMEOUT = 600
REPLAYS = 3


def build():
    env = dict(os.environ, CARGO_TARGET_DIR=str(TARGET))
    subprocess.run(
        ["cargo", "build", "--release", "--manifest-path", str(HERE / "Cargo.toml")],
        env=env,
        check=True,
    )


def run(args):
    t0 = time.time()
    try:
        p = subprocess.run([str(BIN), *args], capture_output=True, text=True, timeout=TIMEOUT)
    except subprocess.TimeoutExpired as e:
        out = (e.stdout or "") + (e.stderr or "")
        return {"out": out, "secs": time.time() - t0, "timeout": True, "code": None}
    return {
        "out": p.stdout + p.stderr,
        "secs": time.time() - t0,
        "timeout": False,
        "code": p.returncode,
    }


def field(out, key):
    m = re.search(rf"^{key}: (.*)$", out, re.M)
    return m.group(1).strip() if m else None


def int_field(out, key):
    v = field(out, key)
    return int(v) if v is not None and v.isdigit() else None


def caveat_keys(out):
    keys = []
    for line in out.splitlines():
        if line.startswith("note: "):
            keys.append(line[6:].split(":")[0])
    return keys


def verdict(r):
    if r["timeout"]:
        return "timeout"
    res = field(r["out"], "RESULT")
    if res == "FAILED":
        return "failed"
    if res == "PASSED":
        return "passed"
    return "crashed"


def anomaly_of(r, stage):
    if r["timeout"]:
        return f"{stage}: timeout after {TIMEOUT}s"
    if field(r["out"], "RESULT") is None:
        tail = r["out"].strip().splitlines()[-3:]
        return f"{stage}: no RESULT line (exit {r['code']}): {' / '.join(tail)}"
    return None


def episode(body, index, seed):
    db = tempfile.mkdtemp(prefix=f"livesets-{body}-")
    rec = {
        "body": body,
        "episode": index,
        "seed": seed,
        "discover_found": False,
        "discover_executions": None,
        "discover_secs": None,
        "discover_caveats": [],
        "blob": None,
        "blob_present": False,
        "blob_nd": None,
        "timelines": None,
        "lengths": None,
        "reuse_reproduced": None,
        "reuse_executions": None,
        "reuse_secs": None,
        "reuse_caveats": [],
        "replays": [],
        "anomalies": [],
    }
    try:
        d = run([f"discover-{body}", db, str(seed)])
        rec["discover_verdict"] = verdict(d)
        rec["discover_found"] = verdict(d) == "failed"
        rec["discover_executions"] = int_field(d["out"], "EXECUTIONS")
        rec["discover_secs"] = round(d["secs"], 3)
        rec["discover_caveats"] = caveat_keys(d["out"])
        rec["discover_panic"] = field(d["out"], "PANIC")
        a = anomaly_of(d, "discover")
        if a:
            rec["anomalies"].append(a)
        if not rec["discover_found"]:
            return rec

        m = BLOB_RE.search(d["out"])
        if m:
            rec["blob"] = m.group(1)
            rec["blob_present"] = True
            b = run(["blobinfo", rec["blob"]])
            tl = field(b["out"], "TIMELINES")
            rec["timelines"] = int(tl) if tl and tl.isdigit() else tl
            lengths = field(b["out"], "LENGTHS")
            rec["lengths"] = [int(x) for x in lengths.split(",") if x] if lengths else None
            nd = field(b["out"], "ND")
            rec["blob_nd"] = None if nd is None else nd == "true"
            a = anomaly_of(b, "blobinfo") if tl is None else None
            if a:
                rec["anomalies"].append(a)
        else:
            rec["anomalies"].append("discover: failed but no reproduce_failure blob printed")

        u = run([f"reuse-{body}", db, str(seed)])
        rec["reuse_verdict"] = verdict(u)
        rec["reuse_reproduced"] = verdict(u) == "failed"
        rec["reuse_executions"] = int_field(u["out"], "EXECUTIONS")
        rec["reuse_secs"] = round(u["secs"], 3)
        rec["reuse_caveats"] = caveat_keys(u["out"])
        a = anomaly_of(u, "reuse")
        if a:
            rec["anomalies"].append(a)

        if rec["blob"]:
            for k in range(REPLAYS):
                p = run([f"replay-{body}", rec["blob"]])
                rec["replays"].append(
                    {
                        "verdict": verdict(p),
                        "reproduced": verdict(p) == "failed",
                        "executions": int_field(p["out"], "EXECUTIONS"),
                        "secs": round(p["secs"], 3),
                        "caveats": caveat_keys(p["out"]),
                        "stale": "no longer reproduces" in p["out"],
                    }
                )
                a = anomaly_of(p, f"replay[{k}]")
                if a:
                    rec["anomalies"].append(a)
    finally:
        shutil.rmtree(db, ignore_errors=True)
    return rec


def median(xs):
    xs = [x for x in xs if x is not None]
    return f"{statistics.median(xs):g}" if xs else "n/a"


def rate(num, den):
    return f"{num}/{den}" if den else "n/a"


def summarize(body, recs):
    n = len(recs)
    found = [r for r in recs if r["discover_found"]]
    with_blob = [r for r in found if r["blob_present"]]
    reuse_done = [r for r in found if r["reuse_reproduced"] is not None]
    reuse_ok = [r for r in reuse_done if r["reuse_reproduced"]]
    replays = [p for r in recs for p in r["replays"]]
    replay_ok = [p for p in replays if p["reproduced"]]
    timelines = Counter(str(r["timelines"]) for r in with_blob)
    first_len = [r["lengths"][0] for r in with_blob if r["lengths"]]
    caveats = Counter(k for r in found for k in r["discover_caveats"])
    reuse_caveats = Counter(k for r in reuse_done for k in r["reuse_caveats"])
    replay_caveats = Counter(k for p in replays for k in p["caveats"])
    anomalies = [f"ep{r['episode']}: {a}" for r in recs for a in r["anomalies"]]

    def hist(c):
        return ", ".join(f"{k}: {v}" for k, v in sorted(c.items())) or "none"

    lines = [
        f"### {body} ({n} episodes)",
        "",
        "| metric | value |",
        "|---|---|",
        f"| discovery rate | {rate(len(found), n)} |",
        f"| median discovery executions | {median([r['discover_executions'] for r in found])} |",
        f"| median discovery seconds | {median([r['discover_secs'] for r in found])} |",
        f"| blob-present rate | {rate(len(with_blob), len(found))} |",
        f"| timelines histogram (count: episodes) | {hist(timelines)} |",
        f"| median first-timeline length | {median(first_len)} |",
        f"| reuse reproduction rate | {rate(len(reuse_ok), len(reuse_done))} |",
        f"| median reuse executions | {median([r['reuse_executions'] for r in reuse_ok])} |",
        f"| blob replay reproduction rate | {rate(len(replay_ok), len(replays))} |",
        f"| median replay executions | {median([p['executions'] for p in replay_ok])} |",
        f"| discovery caveat keys | {hist(caveats)} |",
        f"| reuse caveat keys | {hist(reuse_caveats)} |",
        f"| replay caveat keys | {hist(replay_caveats)} |",
        f"| anomalies | {len(anomalies)} |",
        "",
    ]
    for a in anomalies:
        lines.append(f"- {a}")
    if anomalies:
        lines.append("")
    return "\n".join(lines)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--episodes", type=int, default=20)
    ap.add_argument("--bodies", default=",".join(BODIES))
    ap.add_argument("--out", default=str(HERE / "results.jsonl"))
    ap.add_argument("--no-build", action="store_true")
    args = ap.parse_args()
    if not args.no_build:
        build()
    bodies = [b for b in args.bodies.split(",") if b]
    out_path = Path(args.out)
    out_path.parent.mkdir(parents=True, exist_ok=True)
    summaries = []
    with open(out_path, "w") as f:
        for body in bodies:
            body_index = BODIES.index(body)
            recs = []
            for i in range(args.episodes):
                rec = episode(body, i, i + 1000 * body_index)
                recs.append(rec)
                f.write(json.dumps(rec) + "\n")
                f.flush()
                print(
                    f"[{body} ep{i}] found={rec['discover_found']} exec={rec['discover_executions']} "
                    f"blob={rec['blob_present']} tl={rec['timelines']} reuse={rec['reuse_reproduced']} "
                    f"replays={sum(p['reproduced'] for p in rec['replays'])}/{len(rec['replays'])}"
                    + (f" ANOMALIES={rec['anomalies']}" if rec["anomalies"] else ""),
                    file=sys.stderr,
                    flush=True,
                )
            summaries.append(summarize(body, recs))
    print("\n".join(summaries))


if __name__ == "__main__":
    main()
