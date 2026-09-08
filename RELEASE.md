RELEASE_TYPE: minor

This release changes how hegeltest links Hegel's engine. The engine crate (`hegeltest-c`) is no longer a Rust dependency of `hegeltest`: by default the build script compiles it into the `libhegel_c` shared library and your tests load it at runtime, the same way every other language binding consumes the engine. Its dependencies therefore no longer appear in your cargo graph, where they were subject to feature unification and could even change type inference in unrelated code ([#442](https://github.com/hegeldev/hegel-rust/issues/442)).

`cargo test` and `cargo run` work unchanged. What changes is running a hegeltest binary outside cargo — a deployed `#[hegel::main]` fuzzer, a test binary copied to another machine — which now needs `libhegel_c` shipped next to the executable, its directory named in the `HEGEL_C_LIB_DIR` environment variable, or a copy findable by the platform's own library search (`LD_LIBRARY_PATH` and friends), tried in that order. Whichever copy is found must be the exact engine version this hegeltest release was built against. A mismatched library is refused on load with an error naming both versions. To keep self-contained binaries instead, enable the new `static-engine` feature, which links the engine in as a Rust dependency exactly as before, including its dependency tree:

```toml
hegeltest = { version = "0.40.0", features = ["static-engine"] }
```

Builds without access to crates.io need one of the same escape hatches: when there is no local engine checkout the build script fetches the pinned `hegeltest-c` source from crates.io, so either enable `static-engine` or set `HEGEL_C_LIB_DIR` (it also works at build time) to a directory containing a prebuilt library. Targets that cannot load shared libraries at runtime, such as statically linked musl, need `static-engine`.

This release also removes the internal `__bench` feature (the engine microbenchmarks moved into `hegeltest-c`) and drops the unused `crc32fast`, `dashu-int`, `miniz_oxide`, and `rustc-hash` dependencies. `rand` is now a dependency only under the `rand` feature, and `tempfile` is now test-only.
