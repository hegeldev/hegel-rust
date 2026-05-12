RELEASE_TYPE: minor

This release updates `#[derive(DefaultGenerator)]` for enums to generate snake_case helper APIs for data variants. For example, a `Record` variant now gets `.record(...)` and `.default_record()` helpers, so crates that deny `non_snake_case` no longer need lint suppressions just to derive a generator.

The previous PascalCase helper names remain available as hidden compatibility aliases, so existing callers keep compiling.
