RELEASE_TYPE: minor

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
