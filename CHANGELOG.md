# Changelog

## 0.1.4 - 2026-03-18

This release adds support for `HealthCheck`. A health check is a proactive error raised by Hegel when we detect your test is likely to have degraded testing power or performance. For example, `FilterTooMuch` is raised when too many test cases are filtered out by the rejection sampling of `.filter()` or `assume()`.

Health checks can be suppressed with the new `suppess_health_check` setting.

## 0.1.3 - 2026-03-18

Add a `#[hegel::composite]` macro to define composite generators:


```rust
use hegel::{TestCase, composite, generators};

#[derive(Debug)]
struct Person {
    age: i32,
    has_drivers_license: bool,
}

#[composite]
fn persons(tc: TestCase) -> Person {
    let age: i32 = tc.draw(generators::integers().min_value(0).max_value(100));
    let has_drivers_license = age > 18 && tc.draw(generators::booleans());
    Person { age, has_drivers_license }
}
```

## 0.1.2 - 2026-03-17

Include both `hegeltest` and `hegeltest-macros` in a top-level workspace, to ease automated publishing to crates.io.

## 0.1.1 - 2026-03-17

Update our edition from `2021` to `2024`.

## 0.1.0 - 2026-03-16

Initial release!
