# Getting started with Hegel for Rust

This guide walks you through the basics of installing Hegel and writing your first tests.

## Prerequisites

You will need [`uv`](https://github.com/astral-sh/uv) installed and on your PATH.

## Install Hegel

Add `hegel-rust` to your `Cargo.toml` as a dev dependency:

```toml
[dev-dependencies]
hegeltest = "0.1.0"
```

## Write your first test

You're now ready to write your first test. Add the following to your tests:

```rust
use hegel::TestCase;
use hegel::generators::integers;

#[hegel::test]
fn test_integer_self_equality(tc: TestCase) {
    let n = tc.draw(integers::<i32>());
    assert_eq!(n, n); // integers should always be equal to themselves
}
```

Now run your tests. You should see that the test passes.

Let's look at what's happening in more detail. The `#[hegel::test]` attribute runs your test many times (100, by default). The `test_integer_self_equality` function takes a `hegel::TestCase` parameter, which provides a `draw` method for drawing different values. For each test case, the function then asserts that an integer value should be equal to itself.

Next, try a test that fails:

```rust
#[hegel::test]
fn test_integers_always_below_50(tc: TestCase) {
    let n = tc.draw(integers::<i32>());
    assert!(n < 50); // this will fail!
}
```

This test asserts that any integer is less than 50, which is obviously incorrect. Hegel will find a test case that makes this assertion fail, and then shrink it to find the smallest counterexample — in this case, `n = 50`.

To fix this test, you can constrain the integers you generate with the `min_value` and `max_value` functions:

```rust
#[hegel::test]
fn test_bounded_integers_always_below_50(tc: TestCase) {
    let n = tc.draw(integers::<i32>()
        .min_value(0)
        .max_value(49));
    assert!(n < 50);
}
```

Run the test again. It should now pass.

## Use generators

Hegel provides a rich library of generators that you can use out of the box. There are primitive generators, such as `integers`, `floats`, and `strings`, and combinators that allow you to make generators out of other generators, such as `vecs` and `tuples`. 

For example, you can use `vecs` to generate a vector of integers:

```rust
use hegel::generators::vecs;

#[hegel::test]
fn test_append_increases_length(tc: TestCase) {
    let mut vector = tc.draw(vecs(integers::<i32>()));
    let initial_length = vector.len();
    vector.push(tc.draw(integers::<i32>()));
    assert!(vector.len() > initial_length);
}
```

The test then generates more integers and pushes them to the vector, then tests that the resulting vector is longer than the initial one.

You can also define custom generators with the `composite` macro.

For example, say you have a `Person` struct that we want to generate:

```rust
#[derive(Debug)]
struct Person {
    age: i32,
    name: String,
}
```

You can use `composite` to create a `Person` generator for this struct:

```rust
use hegel::generators::text;

#[hegel::composite]
fn generate_person(tc: TestCase) -> Person {
    let age = tc.draw(integers::<i32>());
    let name = tc.draw(text());
    Person { age, name }
}
```

To customize a generator further, you can make calls to `draw` in sequence that use the results of previous `draw`s. For example, say that you extend the `Person` struct to include a `driving_license` boolean field:

```rust
#[derive(Debug)]
struct Person {
    age: i32,
    name: String,
    driving_license: bool,
}
```

You can then draw values for `driving_license` that depend on the `age` field:

```rust
use hegel::generators::booleans;

fn generate_person(tc: TestCase) -> Person {
    let age = tc.draw(integers::<i32>());
    let name = tc.draw(text());
    let driving_license = if age >= 18 {
        tc.draw(booleans())
    } else {
         false
    };
    Person { age, name, driving_license }
```

## Debug your failing test cases

Use the `note` method to attach debug information: 

```rust
use hegel::generators::{self, Generator};

#[hegel::test]
fn test_with_notes(tc: hegel::TestCase) {
    let x = tc.draw(generators::integers::<i64>());
    let y = tc.draw(generators::integers::<i64>());
    tc.note(&format!("x + y = {x + y}, y + x = {y + x}"));
    assert_eq!(x + y, y + x); // commutativity -- always true
}
```

Notes only appear when Hegel replays the minimal failing example.

## Change the number of test cases

By default Hegel runs 100 test cases. To override this, pass the `test_cases` argument to the `test` attribute:

```rust
use hegel::generators::{self, Generator};

#[hegel::test(test_cases = 500)]
fn test_integers_many(tc: hegel::TestCase) {
    let n = tc.draw(generators::integers::<i64>());
    assert_eq!(n, n);
}
```

## Next steps

You've now had a quick tour of the core features of Hegel. Here are some options for what to try next:

- Run `just docs` to build and browse the full API docs locally.
- Look at `tests/` for more usage patterns.
- See our [API docs](https://crates.io/crates/hegeltest).
