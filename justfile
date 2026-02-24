format:
    cargo fmt
    # also run format-nix if we have nix installed
    which nix && just format-nix || true

check-format:
    cargo fmt --check

format-nix:
    nix run nixpkgs#nixfmt -- flake.nix

check-format-nix:
    nix run nixpkgs#nixfmt -- --check flake.nix

lint:
    cargo clippy --all-features --tests -- -D warnings

check-test:
    cargo test --all-features

check-conformance:
    pytest tests/conformance/test_conformance.py --durations=20 --durations-min=1.0

check-coverage:
    # requires:
    # * cargo install cargo-llvm-cov
    # * rustup component add llvm-tools-preview
    cargo llvm-cov --all-features --fail-under-lines 30 --show-missing-lines

docs:
    cargo clean --doc && cargo doc --open --all-features --no-deps

check-docs:
    RUSTDOCFLAGS="-D warnings" cargo doc --all-features --no-deps


# aliases for local developer experience. CI and builds should use the longer names.
alias test := check-test
check: check-format lint check-test check-docs
