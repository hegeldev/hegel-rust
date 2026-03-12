# hegel-rust

A Rust SDK for [Hegel](https://github.com/hegeldev/hegel-core) — universal
property-based testing powered by [Hypothesis](https://hypothesis.works/).

Hegel generates random inputs for your tests, finds failures, and automatically
shrinks them to minimal counterexamples.

## Installation

Add to your `Cargo.toml`:

```toml
[dev-dependencies]
hegel = { git = "https://github.com/hegeldev/hegel-rust" }
```

Hegel requires [`uv`](https://docs.astral.sh/uv/), and automatically installs the hegel server on first use. To override the hegel binary, set the `HEGEL_SERVER_CMD` environment variable.

## Quick Start

```rust
use hegel::generators;

#[hegel::test]
fn test_addition_commutative(tc: hegel::TestCase) {
    let x = tc.draw(generators::integers::<i32>());
    let y = tc.draw(generators::integers::<i32>());
    assert_eq!(x + y, y + x);
}
```

Run with `cargo test` as normal. Hegel generates 100 random input pairs and
reports the minimal counterexample if it finds one.

For a full walkthrough, see [docs/getting-started.md](docs/getting-started.md).

## Development

```bash
just check       # Full CI: lint + docs + tests
just test        # Run tests only
just conformance # Run cross-language conformance tests
```
