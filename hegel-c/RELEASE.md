RELEASE_TYPE: patch

Internal preparation for making the engine core `no_std`-compatible: the engine's hash tables now use `hashbrown` and its floating-point math now goes through the `libm` crate instead of the platform math library, and the unused `crc32fast` dependency is gone. `libm` can round differently from a platform math library in the last bit, so on some platforms fixed-seed runs may generate different values than previous releases. Stored failures are unaffected: database replay is value-based, and seed reproducibility has always been build-specific.
