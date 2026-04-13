# `just` prints bash comments in stdout by default. this suppresses that
set ignore-comments := true

check-tests:
    RUST_BACKTRACE=1 cargo test

check-tests-all-features:
    RUST_BACKTRACE=1 cargo test --all-features

check-tests-minimal-versions:
    # This is an annoyingly specific check and feels like it overly couples CI concerns and check
    # concerns. I don't have a better proposal right now.

    # --locked tells cargo not to update the lockfile. this makes sure we use the lockfile we just generated
    # and don't regenerate it for non-minimal versions.
    HEGEL_RUNNING_TESTS_WITH_RUST_NIGHTLY=1 RUST_BACKTRACE=1 cargo test --locked --all-features

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
    cargo clippy --all-features --tests -- -D warnings
    cargo clippy --manifest-path tests/conformance/rust/Cargo.toml -- -D warnings

check-docs:
    cargo +nightly docs-rs

docs:
    cargo +nightly docs-rs --open

check-nocov-style:
    scripts/check-nocov-style.py

check-lint: check-format check-clippy check-nocov-style

check-coverage:
    # requires cargo-llvm-cov and llvm-tools-preview
    scripts/check-coverage.py

check-conformance:
    cargo build --release --manifest-path tests/conformance/rust/Cargo.toml
    uv run --with 'hegel-core==0.4.1' --with pytest --with hypothesis \
        pytest tests/conformance/test_conformance.py

# these aliases are provided as ux improvements for local developers. CI should use the longer
# forms.
test: check-tests
coverage: check-coverage
lint: check-lint
check: check-lint check-tests check-tests-all-features
