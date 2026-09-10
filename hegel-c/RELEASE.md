RELEASE_TYPE: patch

This patch fixes an encode/decode gap in the Hegel test case format where an encoder would allow a sequence that the decoder rejected ([#477](https://github.com/hegeldev/hegel-rust/issues/477)). This code path was unreachable in normal test execution so this is mostly an internal change.
