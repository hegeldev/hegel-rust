# Changelog

## 0.26.0 - 2026-07-06

This release replaces the CBOR schema protocol with typed draw functions.
`hegel_generate` — which took a CBOR-encoded schema and returned a
CBOR-encoded value — is gone, along with the entire schema vocabulary.
In its place the ABI now exposes one function per foundational generator:

- `hegel_generate_integer` draws an integer in `[min, max]`, and
  `hegel_generate_integer_big` does the same for bounds beyond `int64_t`
  (two's-complement little-endian byte encodings in and out). The big
  variant sign-fills the output buffer beyond the value's minimal
  encoding, so a caller can read the whole buffer as a fixed-width
  two's-complement integer without doing its own sign extension.
- `hegel_generate_float` takes the full float specification directly:
  width (32 or 64), bounds, NaN/infinity policy, exclusive-bound flags,
  and the smallest nonzero magnitude.
- `hegel_generate_bytes` returns an engine-allocated buffer
  (`hegel_generate_bytes_result_t`) that the caller frees with
  `hegel_generate_bytes_result_free`.
- `hegel_generate_boolean` replaces `hegel_primitive_boolean` (same
  signature). It now draws from the handle's own stream, matching every
  other draw; previously it drew from the family's root stream even on a
  cloned handle.
- String generation goes through opaque `hegel_string_generator_t`
  handles built by typed constructors — `hegel_string_generator_text`
  (codec / codepoint bounds / Unicode categories / include & exclude
  characters), `hegel_string_generator_regex` (with an optional text
  generator as its alphabet), `hegel_string_generator_email`,
  `hegel_string_generator_url`, and `hegel_string_generator_domain`.
  Constructors validate all their parameters immediately, so a bad
  pattern or alphabet is reported at construction rather than mid-draw.
  A generator is immutable after construction, may be shared freely
  across test cases and threads, and is released with
  `hegel_string_generator_free`. `hegel_generate_string` draws through a
  handle and returns an engine-allocated, length-prefixed UTF-8 buffer
  (`hegel_generate_string_result_t`; not NUL-terminated, may contain
  interior NULs) that the caller frees with
  `hegel_generate_string_result_free`.
- `hegel_generate_date`, `hegel_generate_time`, and
  `hegel_generate_datetime` return structured values (`hegel_date_t`,
  `hegel_time_t`, `hegel_datetime_t`) instead of ISO-formatted strings;
  `hegel_generate_uuid` writes the UUID's 16 big-endian bytes (with an
  optional forced RFC 4122 version nibble) and `hegel_generate_ipv4` /
  `hegel_generate_ipv6` write the address's network-order bytes (4 and
  16 respectively).

To migrate a binding, replace each schema construction + `hegel_generate`
call with the corresponding typed call. For example, a bounded integer
draw goes from building `{"type": "integer", "min_value": 0,
"max_value": 100}` as CBOR and decoding the CBOR response to:

```c
int64_t n;
hegel_result_t rc = hegel_generate_integer(ctx, tc, 0, 100, &n);
```

Compound client-side generators (tuples, lists, dictionaries, unions)
should compose the typed draws using the existing span
(`hegel_start_span`/`hegel_stop_span`) and collection
(`hegel_new_collection`/`hegel_collection_more`) primitives, which are
unchanged. New `hegel_label_t` values document the spans the engine now
emits internally around its own draws (`HEGEL_LABEL_REGEX` through
`HEGEL_LABEL_STRING`).

Failure databases and reproduce blobs written by earlier versions will
not replay against generators using the new draw functions (the database
has never been stable across upgrades).

## 0.25.0 - 2026-07-06

This release changes `hegel_test_case_clone` to hand out an *independent
stream* of the test case rather than a view onto the same choice sequence.
A clone still shares the test case's outcome — `hegel_mark_complete` on any
handle completes the whole family, and the choice budget is shared — but it
generates from its own choice sequence, so clones can be driven
concurrently from different threads without perturbing each other, and the
values every stream produces are deterministic under replay and shrink
correctly. Previously concurrent clone draws interleaved into one shared
sequence, which was explicitly non-deterministic.

Each cloned stream is recorded as a single choice in the stream it was
cloned from, so cloning now consumes one choice position on the source
handle, takes the source handle's lock like a draw (it can return
`HEGEL_E_CONCURRENT_USE` on contention), and fails with
`HEGEL_E_ALREADY_COMPLETE` once the test case has completed, where it
previously succeeded and returned a dead handle. Reproduce blobs now encode
the cloned streams' choices alongside their parent's, so blobs from tests
that clone are not readable by older libhegel versions.

Collections, variable pools, and state machines remain shared across the
family — ids from one handle work on any other — but concurrent use of one
such object from two streams makes the affected values scheduling-dependent.

## 0.24.0 - 2026-07-03

This release adds primitives for cloning test-case handles, and clears up the semantics of concurrent use of test cases so that a single test-case handle may not be used concurrently, but clones may. In addition, it changes all of the handle types to be caller-owned and freed by the caller.

This is a breaking change for callers of `hegel_next_test_case`. Previously a run-owned handle was freed by the run, and calling `hegel_test_case_free` on it returned `HEGEL_E_INVALID_HANDLE`; now the caller owns it and must free it.
Run results and failures follow the same caller-owned rule, which is also breaking.
