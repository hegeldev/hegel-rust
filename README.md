# Hegel for Rust

> [!IMPORTANT]
> If you've found this repository, congratulations! You're getting a sneak peek at an upcoming property-based testing library from [Antithesis](https://antithesis.com/), built on [Hypothesis](https://hypothesis.works/).
>
> We are still making rapid changes and progress.  Feel free to experiment, but don't expect stability from Hegel just yet!

## Installation

Add `hegel-rust` to your `Cargo.toml` as a dev dependency:

```toml
[dev-dependencies]
hegeltest = "0.1.0"
```

Hegel requires either:

* [`uv`](https://docs.astral.sh/uv/) on your system,
* or `HEGEL_SERVER_COMMAND` set to the path of a hegel-core binary.

## Quickstart

Here's a quick example of how to write a Hegel test:

```rust
use hegel::generators;

#[hegel::test]
fn test_addition_commutative(tc: hegel::TestCase) {
    let x = tc.draw(generators::integers::<i32>());
    let y = tc.draw(generators::integers::<i32>());
    assert_eq!(x + y, y + x);
}
```

See [docs/getting-started.md](docs/getting-started.md) for more on how to use Hegel.