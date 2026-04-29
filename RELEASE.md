RELEASE_TYPE: minor

`one_of` and the `#[derive(Generate)]` enum implementation no longer wrap children in tagged tuples; they rely on the new protocol contract where the server emits `[index, value]` for `one_of` schemas. Requires hegel >= 0.5.0; the pinned `hegel-core` is bumped to [0.5.0](https://github.com/hegeldev/hegel-core/releases/tag/v0.5.0).
