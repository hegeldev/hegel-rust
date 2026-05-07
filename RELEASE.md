RELEASE_TYPE: patch

Bump our pinned hegel-core to [0.8.0](https://github.com/hegeldev/hegel-core/releases/tag/v0.8.0), incorporating the following changes:

> This release adds support for the `phases` parameter in the `run_test` protocol message,
> allowing clients to control which Hypothesis phases run (e.g. `generate`, `shrink`,
> `reuse`, `target`, `explicit`, `explain`).
>
> — [v0.7.0](https://github.com/hegeldev/hegel-core/releases/tag/v0.7.0)

> This patch adds support for the `uuid` schema type, exposing Hypothesis's
> `uuids` strategy. UUIDs are returned to the client as strings in the
> canonical hyphenated form. An optional `version` field selects a specific
> UUID version (1-5).
>
> — [v0.7.1](https://github.com/hegeldev/hegel-core/releases/tag/v0.7.1)

> This patch bumps `PROTOCOL_VERSION` to `0.14`. The previous release (0.7.1) added the `uuid` schema type without bumping the protocol, so client libraries had no way to negotiate "this server understands `uuid`" at handshake. Bumping now lets clients that emit `{"type": "uuid"}` schemas require protocol `0.14` and fail cleanly against older servers instead of getting an `Unsupported schema` error at draw time.
>
> — [v0.8.0](https://github.com/hegeldev/hegel-core/releases/tag/v0.8.0)
