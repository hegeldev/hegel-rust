# 020: the engine's graph measured against the pool

Status: in progress, 2026-09-21, against `6f202f7e` (decision 78) plus the harness's
`__bench::blob_graph` seam.

Question: decision 78 replaced the pool of timelines (decisions 74–77) with the
counterexample graph, built into the engine in turn 16 of the takeover. Experiments
017–019 measured graphs and a graph shrinker *outside* the engine, against the pool's
stored blobs and its harness-level costs. What does the engine build cost and deliver on
the same bodies, through the real pipeline — discovery, confirmation, shrink, persist,
reuse, blob replay — against the pool's measured numbers (016 campaign 8, 017 Part A)
and the harness-level graph numbers (019)?

## Setup

Harness: `experiments/graph-live/` — 016's `live-set` harness carried to the graph era
(standalone frontend crate + `drive.py`, one process per run). Per episode, as in 016: a
fresh temp database; `discover` (Generate + Shrink, `print_blob`); `blobinfo` on the
blob; `reuse` on the same database (Reuse + Shrink); three `replay`s of the blob via
`reproduce_failure`. The body counts its executions in a process-global counter and
records the count at its first failure, so the cost after discovery is the total minus
it. Seeds are 016's (`episode + 1000 × body index`, 016's body order kept). Discovery and
reuse run at Debug verbosity so the driver can read the engine's `nd shrink start` /
`nd shrink done` lines (edge counts, anchors, `timed_out`) and count `nd graph accept`s.

    CARGO_TARGET_DIR=/tmp/hegel-exp-target python3 experiments/graph-live/drive.py --jobs 6 --episodes 20 --bodies racy,clone,branch,twobranch --out experiments/graph-live/results-016.jsonl
    CARGO_TARGET_DIR=/tmp/hegel-exp-target python3 experiments/graph-live/drive.py --jobs 6 --episodes 10 --bodies kblock2,…,kshift6 --out experiments/graph-live/results-017a.jsonl
    CARGO_TARGET_DIR=/tmp/hegel-exp-target python3 experiments/graph-live/drive.py --jobs 6 --episodes 10 --bodies block4,block8,shift4,shift8,list4,list8,loop4,loop8 --out experiments/graph-live/results-019.jsonl

Bodies, three families:

- **016's**, unchanged: `racy` (the concurrent lost-update counter), `clone` (the
  clone-stream body failing on `x >= 500` every third call), `branch` (`a`, then a hidden
  coin picks `b, x` failing iff `a && b && x >= 60` or `y, z` failing iff both `>= 60`),
  `twobranch` (`a` and two hidden-coin pieces, bool hot iff true / int hot iff `>= 60`,
  fail iff `a` and both hot). 200 test cases.
- **017 Part A's**: `kblock<k>` (`a` and k pieces) and `kshift<k>` (the int arm also draws
  an ignored bool, shifting every later position), k = 2…6, 2000 test cases. These and
  016's have no spans of their own: the arms of a piece are told apart by the engine's
  kind span alone, so they also exercise the same-address-arms limitation of turn 16
  where two arms draw the same kind.
- **019's**, as frontend tests: `block<k>`, `shift<k>` (pieces in PIECE = 1001 spans, the
  shift int arm in an ARM = 1002 span), `list<k>` (`n` drawn in `0..=k`, then n pieces;
  fail iff `a && n >= 1` and all hot) and `loop<k>` (a hidden coin continues at 0.75 up
  to k pieces, then `z`; fail iff `a && hot && z`), k = 4 and 8; 2000 test cases (5000 at
  k = 8, where a fresh case fails at 0.5 × 0.45^8 ≈ 0.08%).

`blobinfo` reads the blob's v3 graph through the seam, enumerates its Start→End paths in
edge order (an edge back onto the current path is counted as cyclic and not followed;
capped at 100 000 paths), and judges each path by the body's own predicate over its
(address, value) steps — 019's method: **right** (a run the body produces and fails on),
**passing** (a run it produces and passes on), **malformed** (no run of the body). It
reports the reachable nodes and edges against the ideal graph (block/kblock: 2 + k nodes,
1 + 2k edges; shift/kshift: 2 + 2k, 1 + 3k; list: 4/4; loop: 3 + k, 1 + 4k), the number
of the failure's structures ("shapes") the right paths cover (2^k; list 2; loop
2^(k+1) − 1), the shortest right path and the longest stored run.

## Campaign 1 — `6f202f7e` as built (`results-019.jsonl`, `results-016.jsonl`)

Run with `--jobs 6` on a machine whose load average was 55–70 from other work; the
harness processes got about half a core each, so seconds are roughly doubled and the
300 s shrink deadline (`MAX_SHRINKING_SECONDS`) bit sooner in executions than it would
unloaded. Executions are the cost measure.

### 019's bodies, 10 episodes each

| body | ideal n/e (shapes) | disc. execs median (after first failure) | shrinks timed out | accepts median | anchor start → done | blob | graphs n/e (count: episodes) | shapes covered | wrong paths | reuse | reuse execs median (re-shrinks) | blob replay (execs) |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| block4 | 6/9 (16) | 31 084 (31 038) | 0/10 | 227 | 0.13 → 0.51 | 10/10 | 6/8: 5, 6/9: 5 | 8: 5, 16: 5 | 0/120 | 10/10 | 2 (2) | 30/30 (1) |
| block8 | 10/17 (256) | 5 128 (4 583) | 0/10 | 0 | 0.06 → 0.68 | **4/10** | 10/16: 1, 10/17: 3 | 128: 1, 256: 3 | 0/896 | 4/10 | 2 (0) | 12/12 (1) |
| shift4 | 10/13 (16) | **277 570** (277 466) | 0/10 | 2 481 | 0.13 → 0.51 | 10/10 | 10/13: 10 | 16: 10 | 0/160 | 10/10 | 2 (0) | 30/30 (1) |
| shift8 | 18/25 (256) | 5 037 (3 961) | 0/8 | 0 | 0.05 → 0.68 | **3/8** | 18/25: 3 | 256: 3 | 0/768 | 3/8 | 2 (0) | 9/9 (1) |
| list4 | 4/4 (2) | 852 (823) | 0/10 | 6 | 0.43 → 0.51 | 10/10 | 4/3: 3, 4/4: 7 | 1: 3, 2: 7 | 0/17 | 10/10 | 2 (1) | 30/30 (1) |
| list8 | 4/4 (2) | 678 (670) | 0/10 | 5.5 | 0.36 → 0.51 | 10/10 | 4/3: 3, 4/4: 7 | 1: 3, 2: 7 | 0/17 | 10/10 | 2 (1) | 30/30 (1) |
| loop4 | 7/17 (31) | 97 120 (97 091) | **3/10** | 2 935 | 0.30 → 0.51 | 10/10 | **3/2: 7**, 7/12: 1, 7/13: 1, 7/15: 1 | 1: 7, 18–21: 3 | 0/65 | 10/10 | **854** (5) | 30/30 (2) |
| loop8 | 11/33 (511) | 85 817 (85 803) | **7/10** | 2 706 | 0.22 → 0.51 | 10/10 | 3/2: 3, 8–11 / 19–31: 7 | 1: 3, 51–481: 7 | 0/1712 | 10/10 | 2.5 (2) | 30/30 (1) |

(shift8 discovered the failure in 8/10 episodes within 5000 cases.) Reproduction from a
stored blob is at the ceiling everywhere and costs one execution; no stored graph holds a
wrong path (no passing or malformed path in 3 755 paths judged). Everything else says the
build is not yet the shrinker 019 measured:

1. **The anchor never climbs past 0.51.** Every discovery's `nd shrink done` anchor is
   0.510 = Wilson LCB(4/4) (0.676 = LCB(6/6) where the confirmation batch seeded higher).
   `GraphShrinker::judge` stopped at the gauntlet's accept and `adopt` seeded the anchor
   from that ledger — the bias decision 54 removed from the pool's shrinker with the
   `ANCHOR_SEED_RUNS` top-up ("a four-straight-fail bar batch seeds 0.51 whatever the
   true rate", `nd/mod.rs`). The graph port dropped the top-up.
2. **With the anchor at 0.51 a deletion that halves the reproduction rate passes.** The
   threshold is 0.8 × 0.51 = 0.41; a graph missing one of a piece's two arms reproduces
   at ~50–70% and clears it after a few clean replays. Then a replay that takes the
   missing arm fails, is grafted back with a fresh random value, the value pass shrinks
   it, the delete pass deletes it again — each an "accept". Hence 227 accepts for a
   9-edge graph (block4), 2 481 (shift4), ~2 900 (loop), 5/10 block4 graphs one arm
   short (8 of 16 shapes), and reuse re-shrinks (016's slow mode) where the reuse replay
   took the missing arm. `loop` is the extreme: the zero-piece run `[T, T]` (3 nodes,
   2 edges) fails whenever the hidden coin stops at once (25%) or the pieces drawn fresh
   past `End` are all hot, ≈ 50% in all — accepted at anchor 0.51, so 10/20 loop episodes
   store it alone, and 10/20 shrinks ran to the deadline churning. This is 016
   campaign 2's finding again ("the delete pass deleted a live branch"), which decision
   75 fixed with deletion by census and 019 (f6) said the graph shrinker needs too;
   decision 78 made deletions need no exercise.
3. **A failure with many structures is reported unconfirmed, without a blob.** block8
   6/10, shift8 5/8: the discovery bar judges the raw single run's graph, whose
   reproduction rate is the failure's *per-shape* rate (0.725⁸ ≈ 7.6%: each piece is
   served with probability ½, else drawn fresh and hot at 0.45), and needs 4 fails in 40.
   The batch grafts every failing replay into the copy it returns but keeps replaying the
   raw graph, so nothing it learns raises the rate it measures. 017 Part A saw the same
   at kshift5/6 under the pool (1–2 of 10); 019 recommended the one-run warm-up (f).
   A failure a fresh run finds in ~1 000 cases should not be "unconfirmed".
4. **Cost.** Where the shrinker converged it cost 3–30× the 019 prototype (block4 31k
   against 019's ~10k for block8 *to the ideal*; shift4 278k against 019's ~9k for
   shift8), all of it the churn of (2).

`list` is right in 7/10 (the span pass does its job) and one arm short in 3/10 — (2)
again. The engine's replay, walk, ties and grafting are sound: no wrong paths, cold
reproduction 100% at one execution, and the graphs that did converge are the ideal.

### 016's bodies, 20 episodes each (as built)

| body | disc. execs median | pool (016 c8) | shrinks timed out | accepts median | anchor done | graphs n/e (count) | wrong paths | reuse (execs; re-shrinks) | blob replay (execs) |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| racy | 2 005 | 1 999 | 0/20 | 0 | 0.76 | 4/6: 5 (15 blobs deterministic) | 0/5 | 20/20 (2; 5) | 60/60 (1) |
| clone | 241 | 2 137 | 0/20 | 0 | 0.15 | 2/1: 20 | 0/20 | 20/20 (4; 0) | 60/60 (1) |
| branch | 819 | 3 301 | 0/20 | 8 | 0.51 | 4/3: 18, 5/5: 1, 5/6: 1 | 3/24 malformed | 20/20 (3; 1) | 60/60 (1) |
| twobranch | **1 197 470** | 4 727 | **13/20** | 15 537 | 0.68 | 4/3: 2, 5/8: 6, 6/9: 2, 6/10: 10 | **64/130 malformed**, 34 cyclic | 20/20 (2; 4, 3 timed out) | 60/60 (1) |

`racy` flipped into ND handling in 5/20 episodes only (016: 14–19/20): under today's load
its lost update mostly reproduced deterministically, so its rows measure the deterministic
path and are comparable between this experiment's two campaigns, not to 016. `clone` is
nine times cheaper than the pool (one edge, nothing to shrink) and `branch` four times,
but `branch` stores the t-arm alone in 18/20 (`[F, 60, 60]` fails whatever `a` is — the
pool's campaign 3 found the same minimum) with 3 malformed paths where it kept both arms.
`twobranch` is the same-address-arms limitation (decision 78) meeting defect (2): a bool
piece after an int piece has the identity `[(BOOL, 1)]`, the same as a bool piece first,
so every graft is a malformed or cyclic path and the shrink churns to the deadline —
1.2 M executions against the pool's 4.7k, 15 537 accepts, half the stored paths runs the
body cannot make.

## Campaign 2 — decision 79 (`results-019-fixed.jsonl`, `results-016-fixed.jsonl`)

Two changes, both restoring a pool-era rule the graph port had dropped: (a) `judge` tops
an accepted candidate's ledger up to `ANCHOR_SEED_RUNS` before its bound seeds the anchor
(decision 54; an unexercised value edit is still rejected at the accept point); (b) the
confirmation batch replays the graph as it grafts failing runs into it (019's warm-up
(f)). Same seeds, same bodies, same load.

### 019's bodies, 10 episodes each

| body | ideal n/e (shapes) | disc. execs median (after first failure) | campaign 1 | shrinks timed out | accepts median | anchor done | blob | graphs n/e | shapes covered | wrong paths | reuse (execs; re-shrinks) | blob replay (execs) |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| block4 | 6/9 (16) | **748** (695) | 31 084 | 0/10 | 9 | 0.839 | 10/10 | 6/9: 10 | 16: 10 | 0/160 | 10/10 (2; 0) | 30/30 (1) |
| block8 | 10/17 (256) | **2 388** (1 569) | 5 128, 4/10 blobs | 0/10 | 19 | 0.839 | **10/10** | 10/17: 10 | 256: 10 | 0/2560 | 10/10 (2; 0) | 30/30 (1) |
| shift4 | 10/13 (16) | **793** (757) | 277 570 | 0/10 | 11.5 | 0.839 | 10/10 | 10/13: 10 | 16: 10 | 0/160 | 10/10 (2; 0) | 30/30 (1) |
| shift8 | 18/25 (256) | 5 011 (1 625) | 5 037, 3/8 blobs | 0/8 | 9 | 0.839 | 4/8 | 18/25: 4 | 256: 4 | 0/1024 | 4/8 (2; 0) | 12/12 (1) |
| list4 | 4/4 (2) | **221** (202) | 852 | 0/10 | 2.5 | 0.839 | 10/10 | 4/4: 10 | 2: 10 | 0/20 | 10/10 (2; 0) | 30/30 (1) |
| list8 | 4/4 (2) | **209** (199) | 678 | 0/10 | 3 | 0.839 | 10/10 | 4/4: 10 | 2: 10 | 0/20 | 10/10 (2; 0) | 30/30 (1) |
| loop4 | 7/17 (31) | **16 625** (16 602) | 97 120, 3 timed out | 0/10 | 53 | 0.839 | 10/10 | 7/15: 1, 7/16: 9 | 23–30 of 31 | 0/271 | 10/10 (2; 0) | 30/30 (1) |
| loop8 | 11/33 (511) | 630 170 (deadline) | 85 817, 7 timed out | **10/10** | 3 655 | 0.839 | 10/10 | 11/30: 1, 11/31: 6, 11/32: 3 | 319–507 of 511 | 0/4034 | 10/10 (2; 0) | 30/30 (1) |

### 016's bodies, 20 episodes each (decision 79)

| body | disc. execs median | campaign 1 | pool (016 c8) | timed out | accepts | anchor done | graphs n/e (count) | wrong paths | reuse (execs; re-shrinks) | blob replay (execs) |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| racy | 1 996 | 2 005 | 1 999 | 0/20 | 0 | 0.839 | 4/6–4/8: 4 (16 deterministic) | 0/7 | 20/20 (2; 4) | 60/60 (1) |
| clone | 241 | 241 | 2 137 | 0/20 | 0 | 0.15 | 2/1: 20 | 0/20 | 20/20 (4; 0) | 60/60 (1) |
| branch | **316** | 819 | 3 301 | 0/20 | 5 | 0.839 | 4/3: 8, 5/6: 12 | 24/56 malformed (12 episodes) | 20/20 (2; 0) | 60/60 (1) |
| twobranch | **730** | 1 197 470 | 4 727 | 0/20 | 7 | 0.839 | 6/10: 20 | **80/160 malformed**, 40 cyclic | 20/20 (2; 0) | 60/60 (1) |

The pool's bodies now cost a third to a tenth of what the pool paid (`branch` 316 against
3 301, `twobranch` 730 against 4 727, `clone` 241 against 2 137) at the same reproduction
(all at the ceiling, one execution per replay, no reuse re-shrink). What the graph gets
wrong on them is the same-address limitation, now as a claim rather than a cost:
`twobranch` covers all four shapes in every episode (the pool: four paths in 5/20) but
stores four malformed paths beside them, `branch` two beside two in 12/20 — the aliased
`[(INT, 0)]` / `[(BOOL, 1)]` states let the walk cross from one arm's draws into the
other's. Reproduction does not suffer because the test's own structure selects the real
path at replay; the stored counterexample over-claims.

- **The churn is gone.** Every episode's anchor ends at `anchor_ceiling()` = 0.839, so
  γ = 1 and a candidate that reproduces at less than 100% is rejected at its first
  unclean replay. block4 costs 748 executions where it cost 31 084, shift4 793 where it
  cost 277 570; accepts are 9–19 where they were hundreds to thousands; block, shift and
  list reach the ideal graph in every episode; no reuse re-shrinks.
- **Many-structure failures confirm.** block8 stores the ideal 256-shape graph in 10/10
  episodes at ~2.4k executions (campaign 1: 4/10, the pool at kblock6: 1–2 shapes of 64).
- **shift8 keeps 4/8 unconfirmed**, and the reason is now visible: every one is the bar's
  fast gate, `failed 0 of 10 replays`. The raw single run reproduces at 0.725⁸ ≈ 8%
  (each piece served with probability ½, else drawn fresh and hot at 0.41–0.5), so the
  gate fires about half the time (0.92¹⁰ = 43%) before anything can be learned; block8
  escapes because its first failure comes early (cases 20–900 in 12 seeds) and generation
  re-sights the origin for another of its five bar attempts, whereas shift8's first
  failure comes at 700–4 300 of 5 000 cases and often gets one. A 12-seed side run: block8
  gated 0/10 in 5 seeds but confirmed on a later attempt in all 12; shift8 gated in 7 and
  ended unconfirmed in 4. Shift's differing arm lengths add a second effect: the
  continuation budget `len + max(4, len/8)` cuts off replays whose arms are longer than
  the stored run's ("Test case stopped: out of data" in 3 of one seed's 10 gate
  replays), a non-failure where block's equal-length runs never overrun.
- **loop is the open problem — 019's (f6).** loop4 ends one edge short of the ideal in
  9/10 (7/16, 27 of 31 shapes) at 17k executions; loop8 runs to the deadline in 10/10 at
  ~630k, 1–3 edges short with 319–507 of 511 shapes, 3 655 accepts. A deletion is
  accepted on 20 clean replays, and 20 replays of a 511-shape graph rarely walk a deep
  edge (piece j is reached with probability 0.75^j): the deletion passes, the replay that
  later walks the edge fails uncleanly and grafts it back, and it is deleted again —
  slow churn at the ceiling anchor instead of fast churn at 0.51. No stored graph holds a
  wrong path. Deletion needs to weigh settlement evidence (an edge no judging replay of
  the candidate could have walked is not certified absent by them), or the walk needs to
  exercise deletions the way value edits are exercised.
