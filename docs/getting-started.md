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

To fix this test, we'll constrain the integers we generate with the `min_value` and `max_value` functions:

```diff
 #[hegel::test]
 fn test_bounded_integers_always_below_50(tc: TestCase) {
-    let n = tc.draw(integers::<i32>();
+    let n = tc.draw(integers::<i32>()
+        .min_value(0)
+        .max_value(49));
     assert!(n < 50);
 }
```

Run the test again. It should now pass.

## Using generators

Hegel provides some generators that you can use out of the box, such as `integers`, `floats`, and `strings`.

<!-- TODO: think about combinators some more -->

You can also define custom generators with the `compose` macro.

For example, say we have a `Person` struct that we want to generate:

```rust
struct Person {
    age: i32,
    name: String,
}
```

You can use `compose` to create a `Person` generator from this struct:

```rust
fn generate_person() {
    hegel::compose!(|tc: TestCase| {
        let age = tc.draw(integers::<i32>());
        let name = tc.draw(strings()); 
        Person { age, name };
    })
}
```

To make more complex custom generators, you can make calls to `draw` in sequence that use the results of previous `draw`s. For example, say that you extend the `Person` struct to include a `driving_license` boolean field:

```diff
 struct Person {
     age: i32,
     name: String,
+    driving_license: bool,
 }
```

You can then draw values for `driving_license` that depend on the `age` field:

```diff
 fn generate_person() {
     hegel::compose!(|tc: TestCase| {
         let age = tc.draw(integers::<i32>());
         let name = tc.draw(strings()); 
+        let driving_license = if (age >= 18) {
+            tc.draw(booleans())
+        } else {
+             false
+         };
+         Person { age, name, driving_license };
     })
 }
```

## Automatically building generators for types

If you don't need custom logic in your generators, as in the first `Person` example above, you can use the `derive` attribute to create a generator automatically:

```rust
#[derive(Generator, Debug)]
struct Person {
    name: String,
    age: u32,
}
```

## Debug your failing test cases

Use the `note` method to attach debug information: 

```rust
use hegel::generators::{self, Generator};

#[hegel::test]
fn test_with_notes(tc: hegel::TestCase) {
    let x = tc.draw(generators::integers::<i64>());
    let y = tc.draw(generators::integers::<i64>());
    tc.note(&format!("trying x={x}, y={y}"));
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

## Guiding generation with target()

> `target()` is not yet available in Hegel for Rust. In other Hegel libraries,
> `target(value, label)` guides the generator toward higher values of a
> numeric metric, useful for finding worst-case inputs. It is planned for
> a future release.

## Next steps

- Run `just docs` to build and browse the full API documentation locally.
- Look at `tests/` for more usage patterns.
- Combine `#[derive(Generator)]` with `.with_<field>()` to generate realistic domain objects.

<!-- TODO: ending -->
