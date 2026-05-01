RELEASE_TYPE: patch

Bump our pinned hegel-core to [0.6.1](https://github.com/hegeldev/hegel-core/releases/tag/v0.6.1), incorporating the following change:

> This patch changes the default Hegel server settings when running inside Antithesis (i.e. when `ANTITHESIS_OUTPUT_DIR` is set in the environment) to disable health checks and database. Health checks are designed for the sort of small fast test you would run in your unit tests and are not sensible defaults for Antithesis, and the database is essentially useless inside Antithesis as replay is done via the fuzzer.
>
> — [v0.6.1](https://github.com/hegeldev/hegel-core/releases/tag/v0.6.1)
