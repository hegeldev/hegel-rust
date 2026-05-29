# `just` prints bash comments in stdout by default. this suppresses that
set ignore-comments := true

check-tests:
    RUST_BACKTRACE=1 cargo test

# `cargo test --all-features` would enable `native`, which swaps the
# entire backend rather than adding capabilities to the server one.
# The "all features" matrix only wants the additive ones — the native
# backend gets its own coverage in `check-tests-native`.
check-tests-all-features:
    RUST_BACKTRACE=1 cargo test --features rand,antithesis,chrono,jiff,serde_json,serde_json_raw_value

# Same as `check-tests-all-features` but drops `antithesis`, which
# emits a `compile_error!` on Windows because the upstream SDK is
# Linux-only.
check-tests-all-features-windows:
    RUST_BACKTRACE=1 cargo test --features rand,chrono,jiff,serde_json,serde_json_raw_value

check-tests-native:
    RUST_BACKTRACE=1 cargo test --features native

check-tests-minimal-versions:
    # This is an annoyingly specific check and feels like it overly couples CI concerns and check
    # concerns. I don't have a better proposal right now.

    # --locked tells cargo not to update the lockfile. this makes sure we use the lockfile we just generated
    # and don't regenerate it for non-minimal versions.
    # Feature list matches `check-tests-all-features`: --all-features would enable `native`,
    # which swaps the backend rather than adding capabilities.
    HEGEL_RUNNING_TESTS_WITH_RUST_NIGHTLY=1 RUST_BACKTRACE=1 cargo test --locked --features rand,antithesis,chrono,jiff,serde_json,serde_json_raw_value

format:
    cargo fmt
    cargo fmt --manifest-path tests/conformance/rust/Cargo.toml
    # also run format-nix if we have nix installed
    @which nix && just format-nix || true

format-nix:
    nix run nixpkgs#nixfmt -- nix/flake.nix

check-format:
    cargo fmt --check
    cargo fmt --manifest-path tests/conformance/rust/Cargo.toml --check

check-format-nix:
    nix run nixpkgs#nixfmt -- --check nix/flake.nix

check-clippy:
    cargo clippy --all-features --all-targets -- -D warnings
    cargo clippy --manifest-path tests/conformance/rust/Cargo.toml --all-targets -- -D warnings

check-docs:
    cargo +nightly docs-rs

docs:
    cargo +nightly docs-rs --open

check-nocov-style:
    scripts/check-nocov-style.py

check-test-modules:
    scripts/check-test-modules.py

check-tests-whole-repo:
    uv run --with pytest pytest tests/whole_repo/

check-lint: check-format check-clippy check-nocov-style check-test-modules

check-coverage:
    # requires cargo-llvm-cov and llvm-tools-preview
    scripts/check-coverage.py

check-conformance:
    cargo build --release --manifest-path tests/conformance/rust/Cargo.toml
    uv run --with 'hegel-core==0.4.1' --with pytest --with hypothesis \
        pytest tests/conformance/test_conformance.py

# Build the libhegel C shared library + checked-in C header.
c-build:
    cargo build -p hegeltest-c --release

# Run the hegel-c smoke tests (Rust integration test that dlopens
# libhegel) and build + run every example C program against both the
# shared (libhegel.so) and static (libhegel.a) builds. The static link
# pulls in the same system libraries Rust's std needs (libdl/pthread/m
# on Linux); --print-link-args from rustc would enumerate them, but
# the set is stable enough to hard-code here.
c-test: c-test-smoke c-test-examples

# Cross-platform half of `c-test`: build the cdylib + run the
# libloading-based smoke tests. Works on Linux, macOS, and Windows.
c-test-smoke:
    # Build the cdylib first so the smoke tests can dlopen it. `cargo test`
    # alone doesn't produce the cdylib artifact (libloading-based tests
    # don't declare a build-link dependency on it).
    cargo build -p hegeltest-c
    cargo test -p hegeltest-c

# Unix-only half of `c-test`: compile + run the example C programs in
# hegel-c/examples/ against both libhegel.so and libhegel.a (and the
# darwin equivalents). The driver is a bash script that assumes a
# Unix-style toolchain (cc + ld); a Windows port is a separate
# follow-up.
[unix]
c-test-examples:
    mkdir -p target/c-examples
    scripts/c-examples-run.sh

[windows]
c-test-examples:
    @echo "Skipping c-test-examples on Windows (bash-based driver, follow-up)"

# Build libhegel with `panic = "abort"` and run the smoke tests + C examples
# against it. This proves no panic is reachable across the FFI boundary on
# the tested paths: under abort, any panic aborts the process, so an
# invalid-schema test that crashed instead of returning HEGEL_E_INVALID_ARG
# would fail the run (the original hegel-java SIGABRT). The cdylib/staticlib
# are built into a separate target dir so they don't clobber the unwind
# artifacts; the smoke-test harness itself stays unwind (so its own
# assertions report normally) and is pointed at the abort artifacts via
# HEGEL_C_LIB_DIR.
[unix]
c-test-abort:
    RUSTFLAGS="-C panic=abort" CARGO_TARGET_DIR=target/abort cargo build -p hegeltest-c
    HEGEL_C_LIB_DIR={{justfile_directory()}}/target/abort/debug cargo test -p hegeltest-c
    mkdir -p target/c-examples
    HEGEL_C_LIB_DIR={{justfile_directory()}}/target/abort/debug scripts/c-examples-run.sh

[windows]
c-test-abort:
    @echo "Skipping c-test-abort on Windows (bash-based driver, follow-up)"

# Regenerate hegel-c/include/hegel.h from the Rust source (no diff check).
c-header:
    HEGEL_C_HEADER_WRITE=1 cargo build -p hegeltest-c

# these aliases are provided as ux improvements for local developers. CI should use the longer
# forms.
test: check-tests
coverage: check-coverage
lint: check-lint
check: check-lint check-tests check-tests-all-features

# Run cargo-insta, installing it first if it's missing. Pinned to the same
# version as the `insta` dev-dependency so snapshot format stays consistent.
INSTA_VERSION := "1.47.2"
insta *ARGS:
    @cargo install --quiet --locked --version {{INSTA_VERSION}} cargo-insta
    cargo insta {{ARGS}}
