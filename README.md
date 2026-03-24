# Hegel for Rust

> [!IMPORTANT]
> We're excited you're checking out Hegel! Hegel is in beta, and we'd love for you to try it and [give any feedback you have](https://github.com/hegeldev/hegel-rust/issues/new).
>
> As part of our beta, we may make breaking changes if it makes Hegel a better property-based testing library. If that instability bothers you, please check back in a few months for a stable release!
>
> See https://hegel.dev/compatibility for more details.

* [Documentation](https://docs.rs/hegeltest)
* [Hegel website](https://hegel.dev)

`hegel-rust` is a property-based testing library for Rust. `hegel-rust` is based on [Hypothesis](https://github.com/hypothesisworks/hypothesis), using the [Hegel](https://hegel.dev/) protocol.

## Installation

To install: `cargo add --dev hegeltest`.

Hegel requires [uv](https://docs.astral.sh/uv/) on your PATH, which we use to install the required [hegel-core](https://github.com/hegeldev/hegel-core) server component. See https://hegel.dev/reference/installation for details.

## Quickstart

Here's a quick example of how to write a Hegel test:

```rust
use hegel::generators::integers;
use hegel::TestCase;

#[hegel::test]
fn test_addition_commutative(tc: TestCase) {
    let x = tc.draw(integers::<i32>());
    let y = tc.draw(integers::<i32>());
    assert_eq!(x + y, y + x);
}
```
