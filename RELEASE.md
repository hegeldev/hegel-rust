RELEASE_TYPE: patch

Bump our pinned hegel-core to [0.4.9](https://github.com/hegeldev/hegel-core/releases/tag/v0.4.9), incorporating the following change:

> This patch removes the unused Unix socket transport from the `hegel` server. The server now always communicates with its client over stdin/stdout, matching how all current libraries spawn it.
>
> — [v0.4.8](https://github.com/hegeldev/hegel-core/releases/tag/v0.4.8)
