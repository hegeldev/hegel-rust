RELEASE_TYPE: patch

This patch moves the [Antithesis](https://antithesis.com/) integration into the engine: the test's location is now passed to libhegel, which writes the verdict to `sdk.jsonl` itself when running inside Antithesis. Nothing changes in what is reported.

With no JSON left to write in the frontend, `serde_json` is now only a dependency when the `serde_json` feature is enabled, instead of always.
