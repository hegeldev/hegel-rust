#!/usr/bin/env python3
"""Summarize graph-replay results (JSON lines) as markdown tables of medians."""
import json
import statistics
import sys
from collections import defaultdict

MERGES = ["exact", "struct", "compat"]
RESCUES = ["random", "skip", "positional", "rejoin"]
BASE = ["fresh", "t0", "pool10", "poolall"]


def med(xs):
    xs = [x for x in xs if x is not None]
    return statistics.median(xs) if xs else float("nan")


def main():
    path = sys.argv[1] if len(sys.argv) > 1 else "results.jsonl"
    groups = defaultdict(list)
    for line in open(path):
        line = line.strip()
        if not line:
            continue
        rec = json.loads(line)
        groups[(rec["body"], rec["confirm"])].append(rec)

    keys = sorted(groups, key=lambda k: (k[0].rstrip("0123456789"), int(k[0].lstrip("abcdefghijklmnopqrstuvwxyz")), k[1]))

    print("### Sizes (medians over trials)\n")
    print("| body | confirm | trials | discovery | confirm fails | pool10 shapes | poolall (shapes) | trie n/e | exact n/e paths | struct n/e paths shapes% wrong% | compat n/e paths shapes% wrong% |")
    print("| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |")
    for body, confirm in keys:
        recs = groups[(body, confirm)]
        k = recs[0]["k"]
        total = 2 ** k

        def g(name, field):
            return med([r["graphs"][name][field] for r in recs])

        def wrong(name):
            return med(
                [
                    100.0 * (r["graphs"][name]["wrong"] + r["graphs"][name]["malformed"]) / max(1, r["graphs"][name]["enumerated"])
                    for r in recs
                ]
            )

        print(
            f"| {body} | {confirm} | {len(recs)} | {med([r['discovery_attempts'] for r in recs]):.0f} | "
            f"{med([r['confirm_fails'] for r in recs]):.0f} | "
            f"{med([r['pool10_shapes'] for r in recs]):.0f}/{total} | "
            f"{med([r['poolall'] for r in recs]):.0f} ({med([r['poolall_shapes'] for r in recs]):.0f}) | "
            f"{med([r['trie'][0] for r in recs]):.0f}/{med([r['trie'][1] for r in recs]):.0f} | "
            f"{g('exact', 'nodes'):.0f}/{g('exact', 'edges'):.0f} {g('exact', 'paths'):.0f} | "
            f"{g('struct', 'nodes'):.0f}/{g('struct', 'edges'):.0f} {g('struct', 'paths'):.0f} {100 * g('struct', 'shapes') / total:.0f}% {wrong('struct'):.1f}% | "
            f"{g('compat', 'nodes'):.0f}/{g('compat', 'edges'):.0f} {g('compat', 'paths'):.0f} {100 * g('compat', 'shapes') / total:.0f}% {wrong('compat'):.1f}% |"
        )

    for idx, title in ((0, "Reproduction rate (% of cold replays that failed; medians over trials)"), (1, "Divergence rate (% of cold replays that left the stored material; medians over trials)")):
        print(f"\n### {title}\n")
        cols = BASE + [f"{m}-{r}" for m in MERGES for r in RESCUES]
        print("| body | confirm | " + " | ".join(cols) + " |")
        print("| --- | --- | " + " | ".join("---" for _ in cols) + " |")
        for body, confirm in keys:
            recs = groups[(body, confirm)]
            r0 = recs[0]["r"]
            cells = [f"{med([100.0 * r['cells'][c][idx] / r0 for r in recs]):.0f}" for c in cols]
            print(f"| {body} | {confirm} | " + " | ".join(cells) + " |")


if __name__ == "__main__":
    main()
