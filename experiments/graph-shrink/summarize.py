#!/usr/bin/env python3
"""Summarize graph-shrink results (JSON lines) as markdown tables of medians."""
import json
import statistics
import sys
from collections import defaultdict

STARTS = ["t0", "exact", "compat"]
JUDGES = ["strict", "lenient", "learn"]


def med(xs):
    xs = [x for x in xs if x is not None]
    return statistics.median(xs) if xs else float("nan")


def body_key(k):
    body, confirm = k
    return (body.rstrip("0123456789"), int(body.lstrip("abcdefghijklmnopqrstuvwxyz")), confirm)


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

    print("### Starting graphs (medians over trials)\n")
    print("| body | confirm | trials | ideal n/e | poolall | t0 cold fail% / clean% | exact n/e paths wrong% | exact cold fail% / clean% | compat n/e paths wrong% | compat cold fail% / clean% |")
    print("| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |")
    for body, confirm in keys:
        recs = groups[(body, confirm)]
        r0 = recs[0]["r"]

        def g(name, field):
            return med([r[f"start-{name}"][field] for r in recs])

        def wrong(name):
            return med([100.0 * (r[f"start-{name}"]["wrong"] + r[f"start-{name}"]["malformed"]) / max(1, r[f"start-{name}"]["enumerated"]) for r in recs])

        def cold(name):
            return f"{med([100.0 * r[f'start-{name}']['cold'][0] / r0 for r in recs]):.0f} / {med([100.0 * r[f'start-{name}']['cold'][2] / r0 for r in recs]):.0f}"

        ideal = recs[0]["ideal"]
        print(
            f"| {body} | {confirm} | {len(recs)} | {ideal[0]}/{ideal[1]} | {med([r['poolall'] for r in recs]):.0f} | {cold('t0')} | "
            f"{g('exact', 'nodes'):.0f}/{g('exact', 'edges'):.0f} {g('exact', 'paths'):.0f} {wrong('exact'):.0f}% | {cold('exact')} | "
            f"{g('compat', 'nodes'):.0f}/{g('compat', 'edges'):.0f} {g('compat', 'paths'):.0f} {wrong('compat'):.0f}% | {cold('compat')} |"
        )

    for k in ks:
        for start in STARTS:
            print(f"\n### Shrinking from `{start}`, K = {k} (medians over trials; cold = % of {recs[0]['r']} cold replays that failed / that failed without leaving the graph)\n")
            print("| body | confirm | judge | start ok | execs | passes | final n/e | ideal | paths | shapes% | wrong% | max n/e | learned | accepts d/c/m/v | cold fail% / clean% |")
            print("| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |")
            for body, confirm in keys:
                recs = groups[(body, confirm)]
                r0 = recs[0]["r"]
                total = 2 ** recs[0]["k"]
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
                        f"{med([100.0 * c['shapes'] / total for c in cs]):.0f}% | {med([100.0 * (c['wrong'] + c['malformed']) / max(1, c['enumerated']) for c in cs]):.0f}% | "
                        f"{med([c['max_nodes'] for c in cs]):.0f}/{med([c['max_edges'] for c in cs]):.0f} | {med([c['learned'] for c in cs]):.0f} | {acc} | "
                        f"{med([100.0 * c['cold'][0] / r0 for c in cs]):.0f} / {med([100.0 * c['cold'][2] / r0 for c in cs]):.0f} |"
                    )


if __name__ == "__main__":
    main()
