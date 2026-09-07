RELEASE_TYPE: patch

This patch adds `generators::btree_sets` and `generators::btree_maps` for generating `BTreeSet` and `BTreeMap` values, with the same `min_size`/`max_size` builders as `hashsets` and `hashmaps`. Both types also implement `DefaultGenerator`, so `gs::default::<BTreeMap<u8, u8>>()` and `#[derive(DefaultGenerator)]` on structs containing them now work ([#445](https://github.com/hegeldev/hegel-rust/issues/445)).
