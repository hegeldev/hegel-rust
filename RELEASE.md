RELEASE_TYPE: minor

This release adds an experimental `native` feature flag that swaps the
hegel-core Python server for an in-process Rust engine.  The minimal
backend supports integer and boolean choice kinds (plus the compound
schemas built on them — tuples, lists, dicts, `one_of`, `sampled_from`),
the database replay / generate / shrink lifecycle, span-mutation, the
non-determinism trie, multi-failure reporting, and per-origin shrinking.

The native backend is intended as the foundation for future PRs; most
generators (`gs::floats`, `gs::text`, `gs::dates`, regex, etc.) and
targeting (`tc.target`) raise `todo!()` until their schema interpreters
land.  Use the default (server) backend for those.
