RELEASE_TYPE: patch

The native backend (`--features native`) now supports byte-string generators.
The `binary` schema (`min_size` / `max_size` bounds) is interpreted natively,
and the shrinker has bytes-specific passes (`shrink_bytes` for shortening
and lowering, `redistribute_bytes_pairs` for moving length between adjacent
bytes nodes). Tests that draw bytes via Hypothesis schemas can now replay
under `--features native`, including `tests/rand/randoms.rs::test_randoms_fill`.
