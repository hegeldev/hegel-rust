# `just` prints bash comments in stdout by default. this suppresses that
set ignore-comments := true

check-tests:
    RUST_BACKTRACE=1 cargo test

check-tests-all-features:
    RUST_BACKTRACE=1 cargo test --all-features

# Same as `check-tests-all-features` but drops `antithesis`, which
# emits a `compile_error!` on Windows because the upstream SDK is
# Linux-only.
check-tests-all-features-windows:
    RUST_BACKTRACE=1 cargo test --features rand,chrono,jiff,serde_json,serde_json_raw_value

check-tests-minimal-versions:
    # This is an annoyingly specific check and feels like it overly couples CI concerns and check
    # concerns. I don't have a better proposal right now.

    # Generate the minimal-versions lockfile (requires nightly), then test against
    # exactly that lockfile (--locked stops cargo regenerating it for non-minimal
    # versions). Note this rewrites the checked-in Cargo.lock; restore it with
    # `git checkout Cargo.lock` afterwards.
    cargo generate-lockfile -Z minimal-versions
    HEGEL_RUNNING_TESTS_WITH_RUST_NIGHTLY=1 RUST_BACKTRACE=1 cargo test --locked --all-features

format:
    cargo fmt
    # also run format-nix if we have nix installed
    @which nix && just format-nix || true

format-nix:
    nix run nixpkgs#nixfmt -- nix/flake.nix

check-format:
    cargo fmt --check

check-format-nix:
    nix run nixpkgs#nixfmt -- --check nix/flake.nix

check-clippy:
    cargo clippy --workspace --all-features --all-targets -- -D warnings

check-docs:
    cargo +nightly docs-rs

docs:
    cargo +nightly docs-rs --open

check-nocov-style:
    scripts/check-nocov-style.py

check-test-modules:
    scripts/check-test-modules.py

check-internal-asserts:
    scripts/check-internal-asserts.py

check-generator-imports:
    scripts/check-generator-imports.py

check-release-script:
    .github/scripts/test_release.py

check-lint: check-format check-clippy check-nocov-style check-test-modules check-internal-asserts check-generator-imports check-release-script

check-coverage:
    # requires cargo-llvm-cov and llvm-tools-preview
    # Force opt-level 0 (overriding `[profile.dev] opt-level` in Cargo.toml):
    # cargo-llvm-cov needs unoptimized code for accurate line attribution, as
    # optimization inlines small functions and drops their coverage regions.
    CARGO_PROFILE_DEV_OPT_LEVEL=0 scripts/check-coverage.py

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

# Run a fast core of the suite under Miri to catch undefined behaviour in the
# native engine and the in-process C-ABI boundary hegeltest drives it through.
# The `test_miri` target exercises the main generator, combinator, derive,
# targeting, assume, and shrinking features end-to-end: every draw flows through
# the same `hegel_generate` / span / collection / target FFI primitives the
# other language bindings use, so this covers the unsafe boundary while staying
# tractable under Miri's interpreter (small example counts, no 100-example
# property loops or budget-exhaustion draws).
#
# The hegeltest-c `c_abi_miri` test target is run too: it drives the C-ABI
# directly — the reference-counted clone/free handle lifecycle (clone,
# free-in-any-order, two clones used concurrently from two threads) plus one
# complete run that generates, fails, and shrinks — which is the pointer/
# aliasing and run-loop logic Miri checks for use-after-free, double-free,
# leaks, and races. It is a dedicated tractable subset; the exhaustive
# `c_abi_inprocess` suite is too slow to interpret in full (chiefly its
# million-draw overrun test) and runs only under normal `cargo test`, and the
# dlopen `smoke` test cannot run under Miri at all (valgrind covers that
# boundary instead).
#
# Requires the nightly toolchain with the miri component:
# `rustup +nightly component add miri`.
#
# CI=1 disables the on-disk failure database (we don't want Miri writing files);
# isolation is disabled because the engine seeds its PRNG from OS entropy.
check-miri:
    CI=1 MIRIFLAGS="-Zmiri-disable-isolation" cargo +nightly miri test --test test_miri
    CI=1 MIRIFLAGS="-Zmiri-disable-isolation" cargo +nightly miri test -p hegeltest-c --test c_abi_miri

# these aliases are provided as ux improvements for local developers. CI should use the longer
# forms.
test: check-tests
coverage: check-coverage
lint: check-lint
miri: check-miri
check: check-lint check-tests check-tests-all-features

# Run cargo-insta, installing it first if it's missing. Pinned to the same
# version as the `insta` dev-dependency so snapshot format stays consistent.
INSTA_VERSION := "1.47.2"
insta *ARGS:
    @cargo install --quiet --locked --version {{INSTA_VERSION}} cargo-insta
    cargo insta {{ARGS}}
