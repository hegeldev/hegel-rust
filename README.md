# Hegel for Rust

> [!IMPORTANT]
> If you've found this repository, congratulations! You're getting a sneak peak at an upcoming property-based testing library from [Antithesis](https://antithesis.com/), built on [Hypothesis](https://hypothesis.works/).
>
> We are still making rapid changes and progress.  Feel free to experiment, but don't expect stability from Hegel just yet!

## Installation

In your `Cargo.toml`:

```toml
[dev-dependencies]
hegel = { git = "https://github.com/hegeldev/hegel-rust" }
```

Hegel requires either:

* [`uv`](https://docs.astral.sh/uv/) on your system (hegel-core is cached in a shared per-user directory),
* or `HEGEL_SERVER_COMMAND` set to the path of a hegel-core binary.

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

See [docs/getting-started.md](docs/getting-started.md) for more.

## Development

```bash
just test        # run tests
just check       # run PR checks: lint + tests + docs
```
