RELEASE_TYPE: patch

This patch bounds zlib decompression when decoding a failure blob. A corrupt or hostile `reproduce_failure` blob could previously force an arbitrarily large allocation. Decoding now rejects payloads that inflate past 16 MiB, and the encoder falls back to the uncompressed encoding for anything that large, so every blob it emits still decodes.
