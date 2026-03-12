# Getting Started with Hegel for Rust

## Install Hegel

Add Hegel to your `Cargo.toml` as a dev dependency:

```toml
[dev-dependencies]
hegel = { git = "ssh://git@github.com/antithesishq/hegel-rust" }
```

The SDK requires the `hegel` CLI on your PATH:

```bash
pip install "git+ssh://git@github.com/antithesishq/hegel-core.git"
```

If you are working inside this repository, `just setup` handles this.

## Write your first test

```rust
use hegel::generators::{self, Generator};

#[hegel::test]
fn test_integers() {
    let n = hegel::draw(&generators::integers::<i64>());
    println!("called with {n}");
    assert_eq!(n, n); // integers are always equal to themselves
}
```

`#[hegel::test]` runs your test many times with different generated inputs.
Inside the body, call `hegel::draw(&generator)` to produce a value. If any
assertion fails, Hegel shrinks the inputs to a minimal counterexample.

By default Hegel runs **100 test cases**. Use the builder API to override this:

```rust
use hegel::generators::{self, Generator};

#[hegel::test(test_cases = 500)]
fn test_integers_many() {
    let n = hegel::draw(&generators::integers::<i64>());
    assert_eq!(n, n);
}
```

## Running in a test suite

Hegel tests use `#[hegel::test]` in place of `#[test]`:

```rust
use hegel::generators::{self, Generator};

#[hegel::test]
fn test_bounded_integers() {
    let n = hegel::draw(&generators::integers::<i32>()
        .with_min(0).with_max(200));
    assert!(n < 50); // this will fail!
}
```

When the test fails, Hegel finds the smallest counterexample — in this case,
`n = 50`.

## Generating multiple values

Call `hegel::draw()` multiple times to produce multiple values in a single test:

```rust
use hegel::generators::{self, Generator};

#[hegel::test]
fn test_multiple_values() {
    let n = hegel::draw(&generators::integers::<i64>());
    let s = hegel::draw(&generators::text());
    assert_eq!(n, n);
    assert!(s.len() >= 0);
}
```

Because generation is imperative, you can generate values at any point —
including conditionally or inside loops.

## Filtering

Use `.filter()` for simple conditions on a generator:

```rust
use hegel::generators::{self, Generator};

#[hegel::test]
fn test_even_integers() {
    let n = hegel::draw(&generators::integers::<i64>()
        .filter(|x| x % 2 == 0));
    assert!(n % 2 == 0);
}
```

When the constraint spans multiple values, use `hegel::assume` inside the
test body:

```rust
use hegel::generators::{self, Generator};

#[hegel::test]
fn test_division() {
    let n1 = hegel::draw(&generators::integers::<i64>());
    let n2 = hegel::draw(&generators::integers::<i64>());
    hegel::assume(n2 != 0);
    // n2 is guaranteed non-zero here
    let q = n1 / n2;
    let r = n1 % n2;
    assert_eq!(n1, q * n2 + r);
}
```

Using bounds and `.map()` is more efficient than `.filter()` or `hegel::assume()`
because they avoid generating values that will be rejected.

## Transforming generated values

Use `.map()` to transform values after generation:

```rust
use hegel::generators::{self, Generator};

#[hegel::test]
fn test_string_integers() {
    let s = hegel::draw(&generators::integers::<i32>()
        .with_min(0).with_max(100)
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
fn test_list_with_valid_index() {
    let n = hegel::draw(&generators::integers::<usize>()
        .with_min(1).with_max(10));
    let lst: Vec<i32> = hegel::draw(&generators::vecs(generators::integers())
        .with_min_size(n).with_max_size(n));
    let index = hegel::draw(&generators::integers::<usize>()
        .with_min(0).with_max(n - 1));
    assert!(index < lst.len());
}
```

You can also use `.flat_map()` for dependent generation within a single
generator expression:

```rust
use hegel::generators::{self, Generator};

#[hegel::test]
fn test_flatmap_example() {
    let (n, lst) = hegel::draw(&generators::integers::<usize>()
        .with_min(1).with_max(5)
        .flat_map(|n| {
            generators::vecs(generators::integers::<i32>())
                .with_min_size(n).with_max_size(n)
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
fn my_test() {
    let b: bool = hegel::draw(&generators::booleans());
    let n: i32 = hegel::draw(&generators::integers::<i32>());    // also i8-i64, u8-u64, usize
    let f: f64 = hegel::draw(&generators::floats::<f64>());      // also f32
    let s: String = hegel::draw(&generators::text());
    let bytes: Vec<u8> = hegel::draw(&generators::binary());
}
```

All numeric generators support `.with_min()` and `.with_max()`. Floats also
support `.exclude_min()`, `.exclude_max()`, `.allow_nan(bool)`, and
`.allow_infinity(bool)`. Text and binary accept `.with_min_size()`/`.with_max_size()`.

### Constants and choices

```rust
use hegel::generators::{self, Generator};

#[hegel::test]
fn my_test() {
    let always_42 = hegel::draw(&generators::just(42));
    let suit = hegel::draw(&generators::sampled_from(vec!["hearts", "diamonds", "clubs", "spades"]));
}
```

### Collections

```rust
use hegel::generators::{self, Generator};
use std::collections::{HashSet, HashMap};

#[hegel::test]
fn my_test() {
    let v: Vec<i32> = hegel::draw(&generators::vecs(generators::integers())
        .with_min_size(1).with_max_size(10));
    let s: HashSet<i32> = hegel::draw(&generators::hashsets(generators::integers())
        .with_max_size(5));
    let m: HashMap<String, i32> = hegel::draw(&generators::hashmaps(
        generators::text().with_max_size(10), generators::integers(),
    ).with_max_size(5));
}
```

### Combinators

```rust
use hegel::generators::{self, Generator};

#[hegel::test]
fn my_test() {
    let pair: (i32, String) = hegel::draw(&generators::tuples2(
        generators::integers(), generators::text(),
    ));
    let triple: (bool, i32, f64) = hegel::draw(&generators::tuples3(
        generators::booleans(), generators::integers(), generators::floats(),
    ));
    let maybe: Option<i32> = hegel::draw(&generators::optional(generators::integers()));

    // Choose between generators (type-erased via one_of! macro)
    let n: i32 = hegel::draw(&hegel::one_of!(
        generators::just(0),
        generators::integers::<i32>().with_min(1).with_max(100),
        generators::integers::<i32>().with_min(-100).with_max(-1),
    ));
}
```

### Formats and patterns

```rust
use hegel::generators::{self, Generator};

#[hegel::test]
fn my_test() {
    let email: String = hegel::draw(&generators::emails());
    let url: String = hegel::draw(&generators::urls());
    let domain: String = hegel::draw(&generators::domains().with_max_length(50));
    let date: String = hegel::draw(&generators::dates());     // YYYY-MM-DD
    let time: String = hegel::draw(&generators::times());      // HH:MM:SS
    let dt: String = hegel::draw(&generators::datetimes());
    let ipv4: String = hegel::draw(&generators::ip_addresses().v4());
    let ipv6: String = hegel::draw(&generators::ip_addresses().v6());
    let pattern: String = hegel::draw(&generators::from_regex(r"[A-Z]{2}-[0-9]{4}").fullmatch());
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
fn test_derived_user() {
    let user: User = hegel::draw(&UserGenerator::new()
        .with_age(generators::integers().with_min(18).with_max(120))
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
// Now hegel::draw(&PointGenerator::new().with_x(...).with_y(...)) works
```

## Debugging with note()

Use `hegel::note()` to attach debug information. Notes only appear when Hegel
replays the minimal failing example:

```rust
use hegel::generators::{self, Generator};

#[hegel::test]
fn test_with_notes() {
    let x = hegel::draw(&generators::integers::<i64>());
    let y = hegel::draw(&generators::integers::<i64>());
    hegel::note(&format!("trying x={x}, y={y}"));
    assert_eq!(x + y, y + x); // commutativity -- always true
}
```

## Guiding generation with target()

> `target()` is not yet available in the Rust SDK. In other Hegel SDKs,
> `target(value, label)` guides the generator toward higher values of a
> numeric metric, useful for finding worst-case inputs. It is planned for
> a future release.

## Next steps

- Run `just docs` to build and browse the full API documentation locally.
- Look at `tests/` for more usage patterns.
- Combine `#[derive(Generator)]` with `.with_<field>()` to generate realistic domain objects.
