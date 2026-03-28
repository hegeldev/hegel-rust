> [!IMPORTANT]
> We're excited you're checking out Hegel! Hegel is in beta, and we'd love for you to try it and [report any feedback](https://github.com/hegeldev/hegel-rust/issues/new).
>
> As part of our beta, we may make breaking changes if it makes Hegel a better property-based testing library. If that instability bothers you, please check back in a few months for a stable release!
>
> See https://hegel.dev/compatibility for more details.

# Hegel for Rust

* [Documentation](https://docs.rs/hegeltest)
* [Hegel website](https://hegel.dev)

`hegel-rust` is a property-based testing library for Rust. `hegel-rust` is based on [Hypothesis](https://github.com/hypothesisworks/hypothesis), using the [Hegel](https://hegel.dev/) protocol.

## Installation

To install: `cargo add --dev hegeltest`.

Hegel requires [uv](https://docs.astral.sh/uv/) on your PATH, which we use to install the required [hegel-core](https://github.com/hegeldev/hegel-core) server component. See https://hegel.dev/reference/installation for details.

## Quickstart

Here's a quick example of how to write a Hegel test:

```rust
use hegel::generators as gs;
use hegel::TestCase;

#[hegel::test]
fn test_addition_commutative(tc: TestCase) {
    let x = tc.draw(gs::integers::<i32>());
    let y = tc.draw(gs::integers::<i32>());
    assert_eq!(x + y, y + x);
}
```

This test will fail when run with `cargo test`! Integer addition panics on overflow in the `test` profile. Hegel will produce a minimal failing test case for us:

```
let x = 1;
let y = 2147483647;
thread 'test_addition_commutative' (2) panicked at examples/readme.rs:8:16:
attempt to add with overflow
```

For a passing test, try:

```rust
#[hegel::test]
fn test_wrapping_addition_commutative(tc: TestCase) {
    let add = i32::wrapping_add;
    let x = tc.draw(gs::integers::<i32>());
    let y = tc.draw(gs::integers::<i32>());
    assert_eq!(add(x, y), add(y, x));
}
```
