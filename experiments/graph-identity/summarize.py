#!/usr/bin/env python3
"""Summarize graph-identity results (JSON lines) as markdown tables of medians."""
import json
import statistics
import sys
from collections import defaultdict

STARTS = ["t0", "exact", "ident"]
JUDGES = ["strict", "lenient", "learn"]


def med(xs):
    xs = [x for x in xs if x is not None]
    return statistics.median(xs) if xs else float("nan")


def body_key(k):
    body, confirm = k
    return (body.rstrip("0123456789"), int(body.lstrip("abcdefghijklmnopqrstuvwxyz")), confirm)


def pct(num, den):
    return 100.0 * num / max(1, den)


def main():
    path = sys.argv[1] if len(sys.argv) > 1 else "results.jsonl"
    groups = defaultdict(list)
    for line in open(path):
        line = line.strip()
        if line:
            rec = json.loads(line)
            groups[(rec["body"], rec["confirm"])].append(rec)
    keys = sorted(groups, key=body_key)
    ks = sorted({int(k.rsplit("-", 1)[1]) for r in groups[keys[0]] for k in r if k.count("-") == 2})
    r0 = groups[keys[0]][0]["r"]

    def cold(cells):
        return " / ".join(f"{med([pct(c['cold'][i], r0) for c in cells]):.0f}" for i in (0, 2, 3))

    def wrong(cells):
        return f"{med([pct(c['wrong'] + c['malformed'], c['enumerated']) for c in cells]):.0f}%"

    print(f"### Starting graphs (medians over trials; cold = % of {r0} cold replays that failed / that failed without leaving the graph / that were misjoined)\n")
    print("| body | confirm | trials | ideal n/e | poolall | " + " | ".join(f"{s} n/e paths shapes% wrong% | {s} cold" for s in STARTS) + " |")
    print("| --- | --- | --- | --- | --- | " + " | ".join("--- | ---" for _ in STARTS) + " |")
    for body, confirm in keys:
        recs = groups[(body, confirm)]
        ideal = recs[0]["ideal"]
        total = recs[0]["shapes_total"]
        row = f"| {body} | {confirm} | {len(recs)} | {ideal[0]}/{ideal[1]} | {med([r['poolall'] for r in recs]):.0f} | "
        for s in STARTS:
            cells = [r[f"start-{s}"] for r in recs]
            row += (
                f"{med([c['nodes'] for c in cells]):.0f}/{med([c['edges'] for c in cells]):.0f} "
                f"{med([c['paths'] for c in cells]):.0f} {med([pct(c['shapes'], total) for c in cells]):.0f}% {wrong(cells)} | {cold(cells)} | "
            )
        print(row)

    for k in ks:
        for start in STARTS:
            print(f"\n### Shrinking from `{start}`, K = {k} (medians over trials; cold = % of {r0} cold replays that failed / that failed without leaving the graph / that were misjoined)\n")
            print("| body | confirm | judge | start ok | execs | passes | final n/e | ideal | paths | shapes% | wrong% | max n/e | learned | misjoined | accepts d/c/m/v | cold |")
            print("| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |")
            for body, confirm in keys:
                recs = groups[(body, confirm)]
                total = recs[0]["shapes_total"]
                ideal = recs[0]["ideal"]
                for judge in JUDGES:
                    cell = f"{start}-{judge}-{k}"
                    cs = [r[cell] for r in recs if cell in r]
                    if not cs:
                        continue
                    ok = sum(c["start_ok"] for c in cs)
                    acc = "/".join(f"{med([c['accepts'][m] for c in cs]):.0f}" for m in ("delete", "contract", "merge", "value"))
                    print(
                        f"| {body} | {confirm} | {judge} | {ok}/{len(cs)} | {med([c['execs'] for c in cs]):.0f} | {med([c['passes'] for c in cs]):.0f} | "
                        f"{med([c['nodes'] for c in cs]):.0f}/{med([c['edges'] for c in cs]):.0f} | {ideal[0]}/{ideal[1]} | {med([c['paths'] for c in cs]):.0f} | "
                        f"{med([pct(c['shapes'], total) for c in cs]):.0f}% | {wrong(cs)} | "
                        f"{med([c['max_nodes'] for c in cs]):.0f}/{med([c['max_edges'] for c in cs]):.0f} | {med([c['learned'] for c in cs]):.0f} | "
                        f"{med([c['misjoined'] for c in cs]):.0f} | {acc} | {cold(cs)} |"
                    )


if __name__ == "__main__":
    main()
