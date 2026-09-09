RELEASE_TYPE: patch

This patch closes a gap between the failure-blob and database encoders and their decoders. The decoders reject choice sequences whose cloned streams nest deeper than the engine's limit of 100, but the encoders accepted such sequences, producing blobs and database entries that could never be read back. The encoders now refuse them too, so every blob or database entry they emit decodes ([#477](https://github.com/hegeldev/hegel-rust/issues/477)). No sequence the engine itself produces is affected: a test case cannot nest clones past that limit.
