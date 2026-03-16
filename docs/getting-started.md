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
```rust

## Define your own generators

Hegel provides some generators that you can use out of the box.

For example, say we have a `Person` structure that we want to generate:
```rust
struct Person {
    age: i32,
    name: string,
}
```

We can define a custom generator with `compose`:


```rust
fn generate_person() {
    hegel::compose!(|tc| {
        Person {
            age: tc.draw(integers::<i32>()),
            name: tc.draw(strings()),
        }
    })
}
```

```rust
fn test_sort_preserves_length(tc: TestCase) {
    let mut vector = draw(&vecs(&integers::<i32>()));
    let initial_length = vector.len();
    vector.sort();
    let sorted_length = vector.len();
    assert_eq!(initial_length, sorted_length);
}
```


```rust
fn test_generator_composition(tc: TestCase) {
    let (x, y) = tc.draw(hegel::compose!(|tc| {
        let x = tc.draw(integers::<i32>().min_value(0).max_value(50));
        let y = tc.draw(integers::<i32>().min_value(x).max_value(100));
        (x, y)
    }));
    assert!(x <= 50);
    assert!(y >= 50 && y <= 100);
}
```

## Filtering

Use `.filter()` for simple conditions on a generator:

```rust
use hegel::generators::{self, Generator};

#[hegel::test]
fn test_even_integers(tc: hegel::TestCase) {
    let n = tc.draw(generators::integers::<i64>()
        .filter(|x| x % 2 == 0));
    assert!(n % 2 == 0);
}
```

When the constraint spans multiple values, use `tc.assume()` inside the
test body:

```rust
use hegel::generators::{self, Generator};

#[hegel::test]
fn test_division(tc: hegel::TestCase) {
    let n1 = tc.draw(generators::integers::<i64>());
    let n2 = tc.draw(generators::integers::<i64>());
    tc.assume(n2 != 0);
    // n2 is guaranteed non-zero here
    let q = n1 / n2;
    let r = n1 % n2;
    assert_eq!(n1, q * n2 + r);
}
```

Using bounds and `.map()` is more efficient than `.filter()` or `tc.assume()`
because they avoid generating values that will be rejected.

## Transforming generated values

Use `.map()` to transform values after generation:

```rust
use hegel::generators::{self, Generator};

#[hegel::test]
fn test_string_integers(tc: hegel::TestCase) {
    let s = tc.draw(generators::integers::<i32>()
        .min_value(0).max_value(100)
        .map(|n| n.to_string()));
    assert!(s.parse::<i32>().unwrap() >= 0);
}
```

## Dependent generation

Because generation is imperative in Hegel, you can use earlier results to
configure later generators directly:

```rust
use hegel::generators::{self, Generator};

#[hegel::test]
fn test_list_with_valid_index(tc: hegel::TestCase) {
    let n = tc.draw(generators::integers::<usize>()
        .min_value(1).max_value(10));
    let lst: Vec<i32> = tc.draw(generators::vecs(generators::integers())
        .min_size(n).max_size(n));
    let index = tc.draw(generators::integers::<usize>()
        .min_value(0).max_value(n - 1));
    assert!(index < lst.len());
}
```

You can also use `.flat_map()` for dependent generation within a single
generator expression:

```rust
use hegel::generators::{self, Generator};

#[hegel::test]
fn test_flatmap_example(tc: hegel::TestCase) {
    let (n, lst) = tc.draw(generators::integers::<usize>()
        .min_value(1).max_value(5)
        .flat_map(|n| {
            generators::vecs(generators::integers::<i32>())
                .min_size(n).max_size(n)
                .map(move |lst| (n, lst))
        }));
    assert_eq!(lst.len(), n);
}
```

## What you can generate

### Primitive types

```rust
use hegel::generators::{self, Generator};

#[hegel::test]
fn my_test(tc: hegel::TestCase) {
    let b: bool = tc.draw(generators::booleans());
    let n: i32 = tc.draw(generators::integers::<i32>());    // also i8-i64, u8-u64, usize
    let f: f64 = tc.draw(generators::floats::<f64>());      // also f32
    let s: String = tc.draw(generators::text());
    let bytes: Vec<u8> = tc.draw(generators::binary());
}
```

All numeric generators support `.min_value()` and `.max_value()`. Floats also
support `.exclude_min()`, `.exclude_max()`, `.allow_nan(bool)`, and
`.allow_infinity(bool)`. Text and binary accept `.min_size()`/`.max_size()`.

### Constants and choices

```rust
use hegel::generators::{self, Generator};

#[hegel::test]
fn my_test(tc: hegel::TestCase) {
    let always_42 = tc.draw(generators::just(42));
    let suit = tc.draw(generators::sampled_from(vec!["hearts", "diamonds", "clubs", "spades"]));
}
```

### Collections

```rust
use hegel::generators::{self, Generator};
use std::collections::{HashSet, HashMap};

#[hegel::test]
fn my_test(tc: hegel::TestCase) {
    let v: Vec<i32> = tc.draw(generators::vecs(generators::integers())
        .min_size(1).max_size(10));
    let s: HashSet<i32> = tc.draw(generators::hashsets(generators::integers())
        .max_size(5));
    let m: HashMap<String, i32> = tc.draw(generators::hashmaps(
        generators::text().max_size(10), generators::integers(),
    ).max_size(5));
}
```

### Combinators

```rust
use hegel::generators::{self, Generator};

#[hegel::test]
fn my_test(tc: hegel::TestCase) {
    let pair: (i32, String) = tc.draw(generators::tuples2(
        generators::integers(), generators::text(),
    ));
    let triple: (bool, i32, f64) = tc.draw(generators::tuples3(
        generators::booleans(), generators::integers(), generators::floats(),
    ));
    let maybe: Option<i32> = tc.draw(generators::optional(generators::integers()));

    // Choose between generators (type-erased via one_of! macro)
    let n: i32 = tc.draw(hegel::one_of!(
        generators::just(0),
        generators::integers::<i32>().min_value(1).max_value(100),
        generators::integers::<i32>().min_value(-100).max_value(-1),
    ));
}
```

### Formats and patterns

```rust
use hegel::generators::{self, Generator};

#[hegel::test]
fn my_test(tc: hegel::TestCase) {
    let email: String = tc.draw(generators::emails());
    let url: String = tc.draw(generators::urls());
    let domain: String = tc.draw(generators::domains().with_max_length(50));
    let date: String = tc.draw(generators::dates());     // YYYY-MM-DD
    let time: String = tc.draw(generators::times());      // HH:MM:SS
    let dt: String = tc.draw(generators::datetimes());
    let ipv4: String = tc.draw(generators::ip_addresses().v4());
    let ipv6: String = tc.draw(generators::ip_addresses().v6());
    let pattern: String = tc.draw(generators::from_regex(r"[A-Z]{2}-[0-9]{4}").fullmatch());
}
```

## Type-directed derivation

`#[derive(Generator)]` creates a builder struct named `<Type>Generator` with
`.new()` and `.with_<field>()` methods:

```rust
use hegel::Generator;
use hegel::generators::{self, Generator as _};

#[derive(Generator, Debug)]
struct User { name: String, age: u32, active: bool }

#[hegel::test]
fn test_derived_user(tc: hegel::TestCase) {
    let user: User = tc.draw(UserGenerator::new()
        .with_age(generators::integers().min_value(18).max_value(120))
        .with_name(generators::from_regex(r"[A-Z][a-z]{2,15}").fullmatch()));
    assert!(user.age >= 18 && user.age <= 120);
}
```

For external types, use `derive_generator!` to generate the same builder:

```rust
use hegel::{derive_generator};
use hegel::generators::{self, Generator};

struct Point { x: f64, y: f64 }
derive_generator!(Point { x: f64, y: f64 });
// Now tc.draw(PointGenerator::new().with_x(...).with_y(...)) works
```

## Debugging with note()

Use `tc.note()` to attach debug information. Notes only appear when Hegel
replays the minimal failing example:

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

## Controlling the number of test cases
By default Hegel runs 100 test cases. Use the builder API to override this:

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
