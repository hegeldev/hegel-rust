RELEASE_TYPE: patch

The native backend (`--features native`) now rejects regex `\u`/`\U` escapes for surrogate codepoints (e.g. `\uD800`) with a clear error instead of panicking, since a Rust `String` cannot contain a surrogate.
