docs:
    cargo clean --doc && cargo doc --open --no-deps

test:
    cargo test

format:
    cargo fmt
