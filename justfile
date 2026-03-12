# don't print bash comments as output during `just` invocation
set ignore-comments := true

check: lint docs test test-all-features

docs:
    cargo clean --doc && cargo doc --open --all-features --no-deps

test:
    RUST_BACKTRACE=1 cargo test

test-all-features:
    RUST_BACKTRACE=1 cargo test --all-features

format:
    cargo fmt
    # also run format-nix if we have nix installed
    @which nix && just format-nix || true

check-format:
    cargo fmt --check

format-nix:
    nix run nixpkgs#nixfmt -- nix/flake.nix

check-format-nix:
    nix run nixpkgs#nixfmt -- --check nix/flake.nix

lint:
    cargo fmt --check
    cargo clippy --all-features --tests -- -D warnings
    RUSTDOCFLAGS="-D warnings" cargo doc --all-features --no-deps

coverage:
    # requires cargo-llvm-cov and llvm-tools-preview
    RUST_BACKTRACE=1 cargo llvm-cov --all-features --fail-under-lines 30 --show-missing-lines

update-hegel-core-version:
    #!/usr/bin/env bash
    set -euo pipefail
    tag=$(gh api repos/hegeldev/hegel-core/releases/latest --jq '.tag_name')
    sed -i '' "s/^const HEGEL_SERVER_VERSION: &str = \".*\"/const HEGEL_SERVER_VERSION: \&str = \"${tag}\"/" src/runner.rs
    sed -i '' "s/refs\/tags\/.*\";/refs\/tags\/${tag}\";/" nix/flake.nix
    nix --extra-experimental-features "nix-command flakes" flake lock ./nix
    echo "Updated HEGEL_SERVER_VERSION to ${tag}"
    # Clear cached install so the next test run picks up the new version
    rm -rf .hegel/venv

build-conformance:
    cargo build --release --manifest-path tests/conformance/rust/Cargo.toml

conformance: build-conformance
    uv run --with "hegel @ git+https://github.com/hegeldev/hegel-core" \
        --with pytest --with hypothesis pytest tests/conformance/test_conformance.py
