RELEASE_TYPE: patch

Bump our pinned hegel-core to [0.4.3](https://github.com/hegeldev/hegel-core/releases/tag/v0.4.3), incorporating the following changes:

> `ListConformance` and `DictConformance` now run twice; once with `{"mode": "basic"}`, and once with `{"mode": "non_basic"}`, indicating the element generators should be basic or non-basic respectively.
>
> — [v0.4.1](https://github.com/hegeldev/hegel-core/releases/tag/v0.4.1)

> Add `crash_after_handshake` and `crash_after_handshake_with_stderr` test modes. These simulate a server that crashes immediately after completing the protocol handshake, allowing client libraries to test crash detection and error reporting without reimplementing the binary protocol in test scripts.
>
> — [v0.4.2](https://github.com/hegeldev/hegel-core/releases/tag/v0.4.2)

> This patch adds a new `OneOfConformance` test, for the `one_of` generator.
>
> This patch also adds recommended integer bound constants (`INT32_MIN`, `INT32_MAX`, `INT64_MIN`, `INT64_MAX`, `BIGINT_MIN`, `BIGINT_MAX`) for use in conformance test setup. Languages with arbitrary-precision integers should use the `BIGINT` bounds to exercise CBOR bignum tag decoding, which is not triggered by the narrower ranges most implementations currently use.
>
> — [v0.4.3](https://github.com/hegeldev/hegel-core/releases/tag/v0.4.3)
