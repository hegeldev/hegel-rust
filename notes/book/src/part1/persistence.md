# Persistence and reproduction

The branch persists exactly one thing: the representation of a failing example.
No rates, miss counters, or nondeterminism status flags are ever written
(decision 8). Every run recomputes its estimates and stands alone, and CI with
the database disabled loses nothing but reuse. Two artefacts share the
encodings: database entries stored under the test's database key, and reproduce
blobs printed in failure reports. Both come in two forms: a v1 entry is a plain
serialized choice sequence, and a v2 entry is `NdReproState`, the replay state a
nondeterministic failure needs. ND-ness is carried by the format itself, so a
stored entry tells the next run how to replay it with no side channel.

## The database layer

`TestCaseDatabase` (hegel-c/src/native/database.rs) is a multi-value key/value
store: each key maps to an unordered set of values, with `fetch`, idempotent
`save`, `delete` (a no-op when the value is absent), and `move_value` (which
inserts at the destination whether or not the value was present at the source).
Failures are silent no-ops, because a non-writable database must never abort
an otherwise-successful run.

`DirectoryTestCaseDatabase` implements it as a directory per key. Keys are
hashed with 64-bit FNV-1a rendered as 16 hex characters, after prepending
`KEY_PREFIX = b"native:"` so the on-disk hashes stay disjoint from any other
store sharing the same root. Values are content-addressed: the file name is the
FNV hex of the value bytes, so `save` is idempotent and two origins whose
entries serialise identically share one file, a fact the Persister's delete
logic respects. The first save under a fresh key records the key bytes under the
bookkeeping key `METAKEYS_NAME = b".hegel-keys"`, and a delete that empties a
key directory removes that record. Writes are atomic: the value goes to a
`<path>.tmp.<pid>.<counter>` sibling and is renamed into place. On failure the
temporary is removed and the target left untouched.

Each test uses two keys: the primary key (the test's database key) holds current
best examples, and the secondary corpus at `sub_key(key, b"secondary")`, the key
bytes plus `.secondary`, holds demoted ones. When no path is configured the
engine defaults the store to `.hegel/examples` (test_runner.rs).

## The v1 entry: serialized choices

`serialize_choices` (database.rs) produces the v1 format:

- 4-byte little-endian u32 choice count
- per choice, a 1-byte tag: 0 Integer, 1 Boolean, 2 Float, 3 Bytes, 4 String, 5
  Clone
- Integer: a 1-byte width sub-tag, always 10 (BigInt) on write, then a 4-byte
  LE length and that many two's-complement LE bytes. Legacy sub-tags 0–9 are
  accepted on read only
- Boolean: one byte
- Float: the raw f64 bit pattern, LE, so `-0.0` and NaN payloads round-trip
  unchanged
- Bytes: u32 LE length, then the raw bytes
- String: u32 LE codepoint count, then 4 bytes per codepoint
- Clone: the cloned stream's child values in the same count-then-entries
  layout, recursively. Only the values are stored, and spans and kinds are
  recreated on replay (decision 32)

`deserialize_choices` rejects truncation, unknown tags, and clone nesting past
`MAX_CLONE_DEPTH`. `deserialize_choices_exact` additionally requires the whole
slice consumed, which is how the v2 decoder validates each embedded timeline.

## The v2 entry: replay state

`NdReproState` (hegel-c/src/native/blob.rs) carries three fields. `timelines`
holds the stored failing timelines with the incumbent first. `entropy` is a u64
seed for the fresh draws a single replay may need past a divergence.
`extension` is the fresh-draw budget beyond the incumbent's flattened length.
`encode_nd_state` writes:

- `ND_STATE_MAGIC = [0xFF; 4]`: an impossible `serialize_choices` count, so a
  pre-v2 reader's `deserialize_choices` rejects the entry as corrupt instead of
  misreading it (decision 8)
- the version byte, `ND_STATE_VERSION = 2`
- entropy as u64 LE, extension as u32 LE
- the timeline count as u32 LE
- per timeline, a u32 LE byte length followed by its `serialize_choices` body

`decode_nd_state` returns `None` on wrong magic or version, truncation, a count
of zero or above `ND_STATE_MAX_TIMELINES = 64`, a timeline body not exactly
consumed, or trailing bytes. The 64 is deliberately looser than the write-side
`POOL_CAP = 10`, so raising the pool cap later never invalidates stored corpora
(decision 42).

The engine assembles the state in `Engine::nd_state_for` (test_runner.rs). The
timelines are the incumbent plus the deduplicated pool, capped by
`pooled_timelines` at `POOL_CAP` total including the incumbent. The entropy is
the FNV-1a hash of the concatenated serialized timelines, so identical state
re-encodes to identical bytes across runs. The extension is
`continuation_budget(len) - len`, where `continuation_budget(len) = len + max(4,
len/8)` (experiment 004).

## Reproduce blobs

A blob is `base64(prefix_byte ++ payload)`. The prefix byte selects the
payload: 0 (`PREFIX_RAW`) is raw `serialize_choices` bytes, 1 (`PREFIX_ZLIB`)
their zlib compression, 2 (`PREFIX_ND_RAW`) raw `encode_nd_state` bytes, and 3
(`PREFIX_ND_ZLIB`) their zlib compression. `encode_failure` and
`encode_nd_failure` compute both forms and keep the compressed one only when it
is strictly shorter and the raw length is within `MAX_DECOMPRESSED_LEN`. The
raw fallback guarantees the decoder's inflation bound never rejects the
encoder's own output. Shrunk counterexamples are tiny, so the zlib header
usually loses and most blobs carry prefix 0. The compression level is
`ZLIB_LEVEL = 6`, the zlib default.

`MAX_DECOMPRESSED_LEN = 16 MiB` bounds decompression so a hostile blob cannot
force an arbitrary allocation (decision 41). It is sized from the largest
choice-only state the decoder would otherwise accept (64 timelines × 8192
choices × roughly 17 bytes per choice, about 8.5 MiB), with comparable headroom
for content-carrying choices. `decode_blob` reverses every step and returns
`None` on any malformation: bad base64, an unknown prefix, a corrupt zlib
stream, a stream inflating past the bound, or a payload the inner decoder
rejects. Callers treat `None` as unreplayable.

## The entry lifecycle

### Mid-run saves: the Persister

The `Persister` (test_runner.rs) does incremental save bookkeeping. Per origin
it remembers the last saved nodes and exact bytes (`last_saved`). Across the run
it tracks every byte string saved this run (`saved_this_run`) and a snapshot of
the primary key's entries at run start (`preexisting`). A save happens when
`needs_save` holds: a forced write, a first sighting, a shortlex-smaller node
sequence, or an equal sort key with different bytes.

The ordering is save-then-delete (decision 44): the new incumbent's bytes are
written to the primary key first, and only then are the bytes they supersede
removed, so the primary key carries the most recent validated incumbent at every
instant and a Ctrl-C or SIGTERM mid-shrink loses nothing. What "removed" means
depends on provenance. A superseded entry that was on the primary key at run
start is demoted to the secondary key. It ended a previous run as someone's
best example, so it takes decision 11's first strike. A superseded same-run save
is deleted, never demoted: it never ended a run as anyone's best example, so it
earned no cross-run staleness strike. Bytes that another origin's last save
still points at are never touched, since content addressing lets two origins
share one file.

When saves fire differs by mode. A deterministic run records every
non-measurement interesting result as it lands, plus the reuse phase's replays
(the `reuse_replays` flag exempts them from the rule that measurement results
are not persisted).
Under ND handling the persistence points are validated events only: confirmation
and adopted gauntlet accepts, via `record_nd_incumbent`, which writes the v2
state. A confirmation during the final replay persists at reconciliation
instead. A backtrack restore goes
through `supersede_nd`: the restored nodes are shortlex-larger than the barred
shrunk save, which the monotone `needs_save` would refuse, so the write is
forced while preserving decision 44's ordering (see [the
lifecycle](lifecycle.md) for backtracking itself).

### Reuse replay and two-strike hygiene

The reuse phase (test_runner.rs, `Phase::Reuse`) fetches the primary key's
entries and sorts them shortlex. When the Generate phase is also enabled it tops
the list up from the secondary corpus to `desired_size = max(ceil(0.1 ×
max_test_cases), 2)` (factor 1.0 without Generate), sampling any shortfall
uniformly at random and shortlex-sorting the extras. Once an interesting result
is found among primary entries, the secondary portion of the sweep stops early.

Each raw entry is decoded: v1 via `deserialize_choices`, v2 via
`decode_nd_state`. Decoding a v2 entry flips the run into ND handling unless
strictness is Error, since only an ND run writes that state (see
[detection](detection.md)). An undecodable entry is deleted from both keys. A
v1 entry under a still-deterministic run replays exactly once. A v2 entry, or
any entry once the run has flipped, replays through `nd_reproduce` with the
reuse budget split per timeline plus `REPRODUCE_SPLICES` splices, stamped for
capture.

The budget arithmetic is `replay_budget(rate, tolerance) = ceil(ln(tolerance) /
ln(1 − rate))` (nd/mod.rs). `reuse_replay_budget()` evaluates it at the target
rate 0.1 (decision 16) and 5% miss tolerance, giving 29 replays. Callers exit on
the first failure, so a live bug costs about 1/p replays and the budget is
spent in full only on stale entries. A flat count of 10 would miss a rate-0.1
bug 35% of the time, which is why the budget is derived rather than fixed
(decision 11).

Miss hygiene is decision 11's two strikes, spread across two runs with no
persisted counters. A deterministic entry that fails to reproduce is deleted
from both keys immediately, because a single exact replay is conclusive. An ND
entry's strike is statistical: a miss after the full budget demotes a primary
entry to the secondary key (strike one) and deletes a secondary entry (strike
two).

A reproduction from a stored entry marks the origin trusted with the reuse
evidence and exempts it from the first-interesting check, since the entry
already replayed it (see [the lifecycle](lifecycle.md) for the Trusted state).
When the stored primary incumbent reproduces node-for-node, `replay_aligned`
holds and the shrink phase is skipped entirely.

### The pre-shrink secondary drain

Before shrinking, a still-deterministic run drains part of the secondary corpus:
entries no shortlex-larger than the largest primary incumbent each get one exact
replay and are then deleted, whatever the outcome. The drain is v1-only and
deterministic-only (decision 40). A v2 entry is skipped. Its hygiene lives in
the reuse phase's budgeted strikes, and under decisions 20 and 24 a pre-shrink
reproduction could change no outcome, so replaying it here would be pure cost.
An undecodable entry is deleted, and a mid-drain flip breaks the loop, because a
flip makes single-replay deletes unsound under decision 11's budget derivation.

### End-of-run reconciliation

After the final replay the run reconciles the primary key. Under ND handling the
new entries are one `encode_nd_state` per interesting origin that passed
confirmation. The filter is `needs_confirmation`, the same predicate
`build_report` uses, so exactly the origins reported with a blob persist. On a
deterministic run they are each origin's serialized choices. All new entries are
saved to the primary key. Every other entry then on the primary key is
dispatched by provenance: an entry in `saved_this_run` but not `preexisting` is
a same-run leftover and is deleted, and anything else is demoted to the
secondary key. The net effect is one secondary deposit per origin per run
(decision 44).

Finally the secondary corpus is capped at `SECONDARY_CORPUS_CAP = 50` entries
per key (five times the reuse phase's default ceiling on secondary sampling),
with the shortlex-largest entries beyond the cap deleted. The cap is a resource
bound outside decision 11's two-strike hygiene (decision 44).

## Blob replay

`hegel_run_start_blob` drives `reproduce_blob` (test_runner.rs). The client
surface is described in [the ABI chapter](abi-frontend.md). An undecodable blob
is the run's error, a `UsageError` saying the blob may be corrupt or from an
incompatible Hegel version.

A v1 blob replays its choices up to `V1_BLOB_REPLAYS = 4` times, each attempt
under the standard continuation budget over the flattened length, each stamped
via `set_should_capture`, stopping at the first failure (decision 59). The
retries exist because one exact no-continuation replay reproduced only 13% of
never-flipped runs' blobs against v2's 100% at p = 0.9 (experiment 009a). Four
continuation attempts bound the worst-case joint escape-then-miss at 1.2 × 10⁻³.

A v2 blob flips the run into ND handling unless strictness is Error, sets
`capture_replays`, and runs `nd_reproduce` over the stored timelines with
per-timeline budget `reuse_replay_budget() / timelines.len()` and
`REPRODUCE_SPLICES = 10` splices (decision 52), but zero fresh generations,
since a fresh case could fail for a reason unrelated to the blob (decision 33).
A reproduction trusts the origin with the accumulated evidence and reports a
failure carrying the trusted caveat and `reproduce_blob: None`, since the caller
already holds the blob. No failure within the budget means the blob is stale.
Database v2 entries replay through this same replay-until-failure path. The
primitive's internal ordering is covered in [the final replay](final-replay.md).

`hegel_test_case_from_blob` remains for embedders as a documented single attempt
(`data_source_for_blob`, hegel-c/src/embed.rs): one test case, no run loop. A v1
blob replays exactly, and a mismatched blob overruns with `HEGEL_E_STOP_TEST`. A
v2 blob replays only the incumbent timeline, seeding the RNG from the stored
entropy and budgeting the incumbent's flattened length plus the extension. The
header steers callers towards `hegel_run_start_blob` for ND blobs.
