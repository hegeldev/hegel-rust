RELEASE_TYPE: minor

Rename `Variables::empty()` to `Variables::is_empty()` to follow Rust naming conventions, and add `Variables::len() -> usize`. The old `empty()` method is removed; callers should use `is_empty()` instead.
