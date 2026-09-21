#!/usr/bin/env python3
"""Drive experiment 020 (the graph-era live experiment): build the harness,
run episodes per body (in parallel with --jobs), write per-episode JSON
lines, and print a markdown summary per body. Same episode pipeline and
seeding as 016's live-set/drive.py."""
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
from concurrent.futures import ProcessPoolExecutor, as_completed
from pathlib import Path

HERE = Path(__file__).resolve().parent
TARGET = Path(os.environ.get("CARGO_TARGET_DIR", "/tmp/hegel-exp-target"))
BIN = TARGET / "release" / "graphlive"
BLOB_RE = re.compile(r'reproduce_failure\("([^"]+)"\)')
SHRINK_START_RE = re.compile(r"^nd shrink start: origin=.*? edges=(\d+) anchor=([\d.]+)", re.M)
SHRINK_DONE_RE = re.compile(
    r"^nd shrink done: origin=.*? edges=(\d+) anchor=([\d.]+) timed_out=(true|false)", re.M
)
ACCEPT_RE = re.compile(r"^nd graph accept: ", re.M)
BODIES_016 = ["racy", "clone", "branch", "twobranch"]
BODIES_017 = [f"kblock{k}" for k in range(2, 7)] + [f"kshift{k}" for k in range(2, 7)]
BODIES_019 = ["block4", "block8", "shift4", "shift8", "list4", "list8", "loop4", "loop8"]
BODIES = BODIES_016 + BODIES_017 + BODIES_019
TIMEOUT = 900
REPLAYS = 3


def ideal(body):
    m = re.match(r"([a-z]+)(\d*)$", body)
    kind, k = m.group(1), int(m.group(2) or 0)
    if body == "twobranch":
        kind, k = "kblock", 2
    if kind in ("kblock", "block"):
        return (2 + k, 1 + 2 * k, 2**k)
    if kind in ("kshift", "shift"):
        return (2 + 2 * k, 1 + 3 * k, 2**k)
    if kind == "list":
        return (4, 4, 2)
    if kind == "loop":
        return (3 + k, 1 + 4 * k, 2 ** (k + 1) - 1)
    return None


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


def shrink_info(out):
    start = SHRINK_START_RE.search(out)
    done = SHRINK_DONE_RE.findall(out)
    last = done[-1] if done else None
    return {
        "shrink_start_edges": int(start.group(1)) if start else None,
        "shrink_start_anchor": float(start.group(2)) if start else None,
        "shrink_done_edges": int(last[0]) if last else None,
        "shrink_done_anchor": float(last[1]) if last else None,
        "shrink_timed_out": (last[2] == "true") if last else None,
        "shrink_runs": len(done),
        "graph_accepts": len(ACCEPT_RE.findall(out)),
    }


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
    db = tempfile.mkdtemp(prefix=f"graphlive-{body}-")
    rec = {
        "body": body,
        "episode": index,
        "seed": seed,
        "discover_found": False,
        "discover_executions": None,
        "first_failure_at": None,
        "discover_secs": None,
        "discover_caveats": [],
        "discover_shrink": None,
        "blob": None,
        "blob_present": False,
        "blob_nd": None,
        "graph": None,
        "reuse_reproduced": None,
        "reuse_executions": None,
        "reuse_secs": None,
        "reuse_caveats": [],
        "reuse_shrink": None,
        "replays": [],
        "anomalies": [],
    }
    try:
        d = run([f"discover-{body}", db, str(seed)])
        rec["discover_verdict"] = verdict(d)
        rec["discover_found"] = verdict(d) == "failed"
        rec["discover_executions"] = int_field(d["out"], "EXECUTIONS")
        rec["first_failure_at"] = int_field(d["out"], "FIRST_FAILURE_AT")
        rec["discover_secs"] = round(d["secs"], 3)
        rec["discover_caveats"] = caveat_keys(d["out"])
        rec["discover_shrink"] = shrink_info(d["out"])
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
            b = run([f"blobinfo-{body}", rec["blob"]])
            nd = field(b["out"], "ND")
            rec["blob_nd"] = None if nd is None else nd == "true"
            if rec["blob_nd"]:
                g = {}
                for key in [
                    "LONGEST", "NODES", "EDGES", "PATHS", "CYCLIC", "SHAPES_ALL", "RIGHT",
                    "PASSING", "MALFORMED", "SHAPES_RIGHT", "MIN_LEN", "MAX_LEN", "MIN_RIGHT_LEN",
                ]:
                    g[key.lower()] = int_field(b["out"], key)
                g["paths_capped"] = field(b["out"], "PATHS_CAPPED") == "true"
                g["judged"] = field(b["out"], "VERDICT") == "judged"
                g["path_list"] = field(b["out"], "PATH_LIST")
                rec["graph"] = g
            a = anomaly_of(b, "blobinfo") if nd is None else None
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
        rec["reuse_shrink"] = shrink_info(u["out"])
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


def hist(c):
    return ", ".join(f"{k}: {v}" for k, v in sorted(c.items(), key=lambda kv: (len(kv[0]), kv[0]))) or "none"


def summarize(body, recs):
    recs = sorted(recs, key=lambda r: r["episode"])
    n = len(recs)
    found = [r for r in recs if r["discover_found"]]
    with_blob = [r for r in found if r["blob_present"]]
    graphs = [r["graph"] for r in with_blob if r["graph"]]
    reuse_done = [r for r in found if r["reuse_reproduced"] is not None]
    reuse_ok = [r for r in reuse_done if r["reuse_reproduced"]]
    replays = [p for r in recs for p in r["replays"]]
    replay_ok = [p for p in replays if p["reproduced"]]
    post_first = [
        r["discover_executions"] - r["first_failure_at"]
        for r in found
        if r["discover_executions"] is not None and r["first_failure_at"]
    ]
    caveats = Counter(k for r in found for k in r["discover_caveats"])
    reuse_caveats = Counter(k for r in reuse_done for k in r["reuse_caveats"])
    replay_caveats = Counter(k for p in replays for k in p["caveats"])
    anomalies = [f"ep{r['episode']}: {a}" for r in recs for a in r["anomalies"]]
    ne = Counter(f"{g['nodes']}/{g['edges']}" for g in graphs)
    shapes_right = Counter(str(g["shapes_right"]) for g in graphs if g["judged"])
    paths_total = sum(g["paths"] or 0 for g in graphs)
    wrong_total = sum((g["passing"] or 0) + (g["malformed"] or 0) for g in graphs if g["judged"])
    passing_total = sum(g["passing"] or 0 for g in graphs if g["judged"])
    malformed_total = sum(g["malformed"] or 0 for g in graphs if g["judged"])
    wrong_eps = sum(1 for g in graphs if g["judged"] and ((g["passing"] or 0) + (g["malformed"] or 0)) > 0)
    timed_out = sum(1 for r in found if (r["discover_shrink"] or {}).get("shrink_timed_out"))
    reuse_timed_out = sum(1 for r in reuse_done if (r["reuse_shrink"] or {}).get("shrink_timed_out"))
    reuse_reshrunk = sum(1 for r in reuse_done if (r["reuse_shrink"] or {}).get("shrink_runs"))
    idl = ideal(body)
    ideal_str = f"{idl[0]}/{idl[1]} ({idl[2]} shapes)" if idl else "n/a"
    at_ideal = sum(1 for g in graphs if idl and g["nodes"] == idl[0] and g["edges"] == idl[1])

    lines = [
        f"### {body} ({n} episodes)",
        "",
        "| metric | value |",
        "|---|---|",
        f"| discovery rate | {rate(len(found), n)} |",
        f"| median discovery executions | {median([r['discover_executions'] for r in found])} |",
        f"| median executions after the first failure | {median(post_first)} |",
        f"| median discovery seconds | {median([r['discover_secs'] for r in found])} |",
        f"| discovery shrinks timed out | {rate(timed_out, len(found))} |",
        f"| median graph accepts (discovery) | {median([(r['discover_shrink'] or {}).get('graph_accepts') for r in found])} |",
        f"| median shrink anchor at start → done | {median([(r['discover_shrink'] or {}).get('shrink_start_anchor') for r in found])} → {median([(r['discover_shrink'] or {}).get('shrink_done_anchor') for r in found])} |",
        f"| blob-present rate | {rate(len(with_blob), len(found))} |",
        f"| ideal graph nodes/edges | {ideal_str} |",
        f"| graph nodes/edges histogram | {hist(ne)} |",
        f"| episodes at the ideal graph | {rate(at_ideal, len(graphs)) if idl else 'n/a'} |",
        f"| median paths | {median([g['paths'] for g in graphs])} |",
        f"| shapes covered by right paths (count: episodes) | {hist(shapes_right)} |",
        f"| wrong paths (passing + malformed) / all paths | {wrong_total} ({passing_total} + {malformed_total}) / {paths_total}, in {wrong_eps} episodes |",
        f"| cyclic edges seen | {sum(g['cyclic'] or 0 for g in graphs)} |",
        f"| median shortest right path length | {median([g['min_right_len'] for g in graphs])} |",
        f"| median longest stored run | {median([g['longest'] for g in graphs])} |",
        f"| reuse reproduction rate | {rate(len(reuse_ok), len(reuse_done))} |",
        f"| median reuse executions | {median([r['reuse_executions'] for r in reuse_ok])} |",
        f"| reuses that re-shrank / timed out | {reuse_reshrunk} / {reuse_timed_out} |",
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
    ap.add_argument("--jobs", type=int, default=1)
    ap.add_argument("--no-build", action="store_true")
    ap.add_argument("--summarize", action="store_true", help="only summarize --out")
    args = ap.parse_args()
    out_path = Path(args.out)
    if args.summarize:
        recs = [json.loads(l) for l in open(out_path) if l.strip()]
        bodies = [b for b in BODIES if any(r["body"] == b for r in recs)]
        print("\n".join(summarize(b, [r for r in recs if r["body"] == b]) for b in bodies))
        return
    if not args.no_build:
        build()
    bodies = [b for b in args.bodies.split(",") if b]
    out_path.parent.mkdir(parents=True, exist_ok=True)
    jobs = [(body, i, i + 1000 * BODIES.index(body)) for body in bodies for i in range(args.episodes)]
    recs = []
    with open(out_path, "w") as f, ProcessPoolExecutor(max_workers=args.jobs) as pool:
        futures = {pool.submit(episode, *job): job for job in jobs}
        for fut in as_completed(futures):
            rec = fut.result()
            recs.append(rec)
            f.write(json.dumps(rec) + "\n")
            f.flush()
            g = rec["graph"] or {}
            print(
                f"[{rec['body']} ep{rec['episode']}] found={rec['discover_found']} exec={rec['discover_executions']} "
                f"first={rec['first_failure_at']} blob={rec['blob_present']} n/e={g.get('nodes')}/{g.get('edges')} "
                f"paths={g.get('paths')} right={g.get('right')} wrong={(g.get('passing') or 0) + (g.get('malformed') or 0)} "
                f"reuse={rec['reuse_reproduced']} rexec={rec['reuse_executions']} "
                f"replays={sum(p['reproduced'] for p in rec['replays'])}/{len(rec['replays'])}"
                + (f" ANOMALIES={rec['anomalies']}" if rec["anomalies"] else ""),
                file=sys.stderr,
                flush=True,
            )
    print("\n".join(summarize(b, [r for r in recs if r["body"] == b]) for b in bodies))


if __name__ == "__main__":
    main()
