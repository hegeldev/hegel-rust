# Changelog

## 0.1.12 - 2026-03-20

This release improves derived default generators:

* Makes the derive method DefaultGenerator, not Generator, as that's what's actually derived.
* Brings the builder methods for derived generators in line with the standard convention, removing the with_ prefix from them.
* Fixes a bug where if you did not have `hegel::Generator` imported, DefaultGenerator would fail to derive.

## 0.1.11 - 2026-03-20

This improves error messages when uv is not installed.

## 0.1.10 - 2026-03-20

Adds support for the on-disk database, which automatically replays failing test.

Also adds the `hegel::Settings` struct to encapsulate settings.

## 0.1.9 - 2026-03-19

This patch bumps the minimum supported protocol version to 0.6.

## 0.1.8 - 2026-03-19

When the hegel server process exits unexpectedly, the library now detects this immediately and fails with a clear error pointing to `.hegel/server.log`, instead of blocking for up to 120 seconds on the socket read timeout.

## 0.1.7 - 2026-03-18

This patch adds support for outputting Hegel events as Antithesis SDK events.

## 0.1.6 - 2026-03-18

This release adds client-side support for reporting flaky test errors to the end user.

## 0.1.5 - 2026-03-18

This release updates the hegel-core version to support the new health checks feature.

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
