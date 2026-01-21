# Hegel Rust SDK

Hegel rust SDK.

## Installation

Add to your `Cargo.toml`:

```toml
[dev-dependencies]
hegel = { git = "ssh://git@github.com/antithesishq/hegel-rust" }
```

The SDK automatically installs the Hegel CLI at compile time if not already on PATH.

## Quick Start

```rust
use hegel::gen::{self, Generate};

#[test]
fn test_addition_commutative() {
    hegel::hegel(|| {
        let x = gen::integers::<i32>().generate();
        let y = gen::integers::<i32>().generate();
        assert_eq!(x + y, y + x);
    });
}
```

Run with `cargo test`.

## Configuration

Use the builder pattern for more control:

```rust
use hegel::{Hegel, Verbosity};
use hegel::gen::{self, Generate};

#[test]
fn test_with_options() {
    Hegel::new(|| {
        let n = gen::integers::<i32>().generate();
        assert!(n + 0 == n);
    })
    .test_cases(500)
    .verbosity(Verbosity::Verbose)
    .run();
}
```

## API Reference

### The Generate Trait

All generators implement `Generate<T>`:

```rust
pub trait Generate<T> {
    fn generate(&self) -> T;
    fn schema(&self) -> Option<Value>;

    fn map<U, F>(self, f: F) -> Mapped<...>;
    fn flat_map<U, G, F>(self, f: F) -> FlatMapped<...>;
    fn filter<F>(self, predicate: F, max_attempts: usize) -> Filtered<...>;
    fn boxed(self) -> BoxedGenerator<T>;
}
```

### Primitives

```rust
use hegel::gen::{self, Generate};

hegel::hegel(|| {
    // Unit
    let _: () = gen::unit().generate();

    // Booleans
    let b: bool = gen::booleans().generate();

    // Constants (with schema support)
    let n: i32 = gen::just(42).generate();
    let s: String = gen::just("hello".to_string()).generate();

    // Constants (without schema, for non-Serialize types)
    let any = gen::just_any(my_value).generate();
});
```

### Numbers

```rust
hegel::hegel(|| {
    // Integers - bounds default to type limits
    let i: i32 = gen::integers::<i32>().generate();
    let bounded: i32 = gen::integers::<i32>().with_min(0).with_max(100).generate();
    let byte: u8 = gen::integers::<u8>().generate();  // automatically 0-255

    // Floating point
    let f: f64 = gen::floats::<f64>().generate();
    let bounded_f: f64 = gen::floats::<f64>()
        .with_min(0.0)
        .with_max(1.0)
        .exclude_min()  // exclusive bounds
        .exclude_max()
        .generate();
});
```

### Strings

```rust
hegel::hegel(|| {
    // Text with optional length constraints
    let s: String = gen::text().generate();
    let bounded: String = gen::text().with_min_size(1).with_max_size(100).generate();

    // Regex patterns (auto-anchored with ^ and $)
    let pattern: String = gen::from_regex(r"[a-z]{3}-[0-9]{3}").generate();

    // Format strings
    let email: String = gen::emails().generate();
    let url: String = gen::urls().generate();
    let domain: String = gen::domains().with_max_length(63).generate();
    let ip: String = gen::ip_addresses().generate();      // IPv4 or IPv6
    let ipv4: String = gen::ip_addresses().v4().generate();
    let ipv6: String = gen::ip_addresses().v6().generate();

    // Date/time (ISO 8601)
    let date: String = gen::dates().generate();       // YYYY-MM-DD
    let time: String = gen::times().generate();       // HH:MM:SS
    let datetime: String = gen::datetimes().generate();
});
```

### Collections

```rust
hegel::hegel(|| {
    // Vectors
    let vec: Vec<i32> = gen::vecs(gen::integers::<i32>()).generate();
    let bounded: Vec<i32> = gen::vecs(gen::integers::<i32>())
        .with_min_size(1)
        .with_max_size(10)
        .unique()  // no duplicates
        .generate();

    // HashSets
    let set: HashSet<i32> = gen::hashsets(gen::integers::<i32>())
        .with_min_size(1)
        .with_max_size(5)
        .generate();

    // HashMaps (string keys only, JSON limitation)
    let map: HashMap<String, i32> = gen::hashmaps(gen::integers::<i32>())
        .with_min_size(1)
        .with_max_size(3)
        .generate();
});
```

### Tuples

```rust
hegel::hegel(|| {
    let pair: (i32, String) = gen::tuples(gen::integers::<i32>(), gen::text()).generate();
    let triple: (bool, i32, f64) = gen::tuples3(
        gen::booleans(),
        gen::integers::<i32>(),
        gen::floats::<f64>()
    ).generate();
});
```

### Combinators

```rust
hegel::hegel(|| {
    // Sample from fixed set
    let color: &str = gen::sampled_from(vec!["red", "green", "blue"]).generate();
    let num: i32 = gen::sampled_from(vec![1, 2, 3, 4, 5]).generate();

    // Choose from multiple generators
    let n: i32 = hegel::one_of!(
        gen::integers::<i32>().with_min(0).with_max(10),
        gen::integers::<i32>().with_min(100).with_max(110),
    ).generate();

    // Optional values
    let opt: Option<i32> = gen::optional(gen::integers::<i32>()).generate();
});
```

### map, flat_map, filter

```rust
hegel::hegel(|| {
    // Transform values
    let squared: i32 = gen::integers::<i32>()
        .with_min(1)
        .with_max(10)
        .map(|x| x * x)
        .generate();

    // Filter values (rejects after max_attempts failures)
    let even: i32 = gen::integers::<i32>()
        .with_min(0)
        .with_max(100)
        .filter(|x| x % 2 == 0, 10)
        .generate();

    // Dependent generation
    let sized_string: String = gen::integers::<usize>()
        .with_min(1)
        .with_max(10)
        .flat_map(|len| gen::text().with_min_size(len).with_max_size(len))
        .generate();
});
```

### Struct Generation with Derive Macro

For types defined in your crate, use the derive macro:

```rust
use hegel::Generate;
use hegel::gen::{self, Generate as _};

#[derive(Generate, Debug)]
struct Person {
    name: String,
    age: u32,
}

#[test]
fn test_person() {
    hegel::hegel(|| {
        // Use generated PersonGenerator with builder methods
        let gen = PersonGenerator::new()
            .with_name(gen::text().with_min_size(1).with_max_size(50))
            .with_age(gen::integers::<u32>().with_min(0).with_max(120));

        let person: Person = gen.generate();
        assert!(person.age <= 120);
    });
}
```

### Enum Generation

Enums with data variants require `serde::Deserialize`:

```rust
use hegel::Generate;
use serde::Deserialize;

// Unit-only enums: no serde needed
#[derive(Generate)]
enum Color {
    Red,
    Green,
    Blue,
}

// Enums with data: need Deserialize
#[derive(Generate, Deserialize)]
enum Message {
    Text(String),
    Number(i32),
}
```

Add serde to your Cargo.toml only if you use enums with data variants:

```toml
[dev-dependencies]
serde = { version = "1.0", features = ["derive"] }
```

### Struct Generation for External Types

For types from other crates, use the `derive_generator!` macro:

```rust
use hegel::derive_generator;
use hegel::gen::{self, Generate};

// Type from another crate
use external_crate::Point;

derive_generator!(Point {
    x: f64,
    y: f64,
});

#[test]
fn test_point() {
    hegel::hegel(|| {
        let gen = PointGenerator::new()
            .with_x(gen::floats::<f64>().with_min(-100.0).with_max(100.0))
            .with_y(gen::floats::<f64>().with_min(-100.0).with_max(100.0));

        let point: Point = gen.generate();
    });
}
```

### Fixed Dictionaries

Generate JSON objects with specific fields:

```rust
use serde_json::Value;

hegel::hegel(|| {
    let gen = gen::fixed_dicts()
        .field("name", gen::text().with_min_size(1))
        .field("age", gen::integers::<u32>())
        .field("active", gen::booleans())
        .build();

    let value: Value = gen.generate();
});
```

### Assumptions

Use `assume()` when generated data doesn't meet preconditions:

```rust
use hegel::assume;

hegel::hegel(|| {
    let person = person_gen.generate();
    assume(person.age >= 18);

    // Test logic for adults only...
});
```

This tells Hegel to try different input data rather than counting as a test failure.

## Debugging

Set `HEGEL_DEBUG=1` to enable debug logging of requests/responses.

## Complete Example

```rust
use hegel::gen::{self, Generate};
use hegel::Generate as DeriveGenerate;
use hegel::assume;

#[derive(DeriveGenerate, Debug)]
struct Order {
    id: String,
    items: Vec<String>,
    total: f64,
}

#[test]
fn test_order_creation() {
    hegel::Hegel::new(|| {
        let gen = OrderGenerator::new()
            .with_id(gen::from_regex(r"ORD-[0-9]{6}"))
            .with_items(gen::vecs(gen::text().with_min_size(1)).with_min_size(1))
            .with_total(gen::floats::<f64>().with_min(0.0).with_max(10000.0));

        let order: Order = gen.generate();

        assert!(order.id.starts_with("ORD-"));
        assert!(!order.items.is_empty());
    })
    .test_cases(500)
    .run();
}

#[test]
fn test_order_total() {
    hegel::hegel(|| {
        let gen = OrderGenerator::new()
            .with_total(gen::floats::<f64>().with_min(0.0));

        let order: Order = gen.generate();
        assume(order.total >= 0.0);

        // Test logic...
    });
}
```

## DefaultGenerator Trait

Types implementing `DefaultGenerator` can be generated without explicit configuration:

```rust
// Built-in implementations for:
// - bool, String
// - i8, i16, i32, i64, u8, u16, u32, u64
// - f32, f64
// - Option<T> where T: DefaultGenerator
// - Vec<T> where T: DefaultGenerator
```

The derive macro automatically uses `DefaultGenerator` for fields when you call `::new()`.
