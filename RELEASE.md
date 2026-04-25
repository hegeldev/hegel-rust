RELEASE_TYPE: minor

This release adds a `DefaultGenerator` impl for `PathBuf`, so `#[derive(Generate)]` works for structs with `PathBuf` fields and `gs::default::<PathBuf>()` returns a generator.
