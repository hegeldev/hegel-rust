# Changelog

## 0.7.4 - 2026-04-20

This patch adds windows support for hegel-rust. It is somewhat experimental, but the full
feature set should work.

## 0.7.3 - 2026-04-20

This adds two new macros to allow more flexible use of hegel.

* `#[hegel::main]` wraps a function as a standalone binary entry point, exposing CLI flags for every Settings option.
* `#[hegel::standalone_function]` rewrites fn(tc: TestCase, args...) into fn(args...) so a property test can be invoked directly.

## 0.7.2 - 2026-04-20

This patch loosens restrictions on using threads in hegel-rust. `TestCase` now implements `Send`
(and already implemented Clone) so data generation can now occur from multiple threads.

> [!IMPORTANT]
>
> This feature should be used only with extreme caution at present. Please consult the `TestCase`
> documentation for details, but it can only be correctly used with extreme care. We intend
> for threading support to get significantly better over time without the API changing,
> and this is an initial release of the intended API with the worst possible implementation
> of it. We are releasing it because even with these caveats it is useful in some cases,
> but it is highly likely not to be ready for you to use yet.

## 0.7.1 - 2026-04-20

This patch adds support for a `repeat` method on test case, for operations that
you want to run repeatedly until they hit an error. Effectively equivalent to
a `loop` that is better optimised for testing.

## 0.7.0 - 2026-04-20

This release adds a `reject` method to `TestCase` and `ExplicitTestCase`. It behaves like `assume(false)`, rejecting the current test input, but returns `!` so the compiler knows that code following the call is unreachable.

```rust
#[hegel::test]
fn my_test(tc: hegel::TestCase) {
    let n: i32 = tc.draw(gs::integers());
    let positive: u32 = match u32::try_from(n) {
        Ok(v) => v,
        Err(_) => tc.reject(),
    };
    // use `positive` here without needing an extra `unreachable!()` branch
}
```

## 0.6.2 - 2026-04-20

Bump our pinned hegel-core to [0.4.4](https://github.com/hegeldev/hegel-core/releases/tag/v0.4.4), incorporating the following changes:

> This patch adds a new `OneOfConformance` test, for the `one_of` generator.
>
> This patch also adds recommended integer bound constants (`INT32_MIN`, `INT32_MAX`, `INT64_MIN`, `INT64_MAX`, `BIGINT_MIN`, `BIGINT_MAX`) for use in conformance test setup. Languages with arbitrary-precision integers should use the `BIGINT` bounds to exercise CBOR bignum tag decoding, which is not triggered by the narrower ranges most implementations currently use.
>
> — [v0.4.3](https://github.com/hegeldev/hegel-core/releases/tag/v0.4.3)

> This release is in support of getting hegel libraries working on Windows. It mostly fixes issues affecting the conformance testing.
>
> Windows support still won't work in individual libraries until they also do work to support it.
>
> — [v0.4.4](https://github.com/hegeldev/hegel-core/releases/tag/v0.4.4)

## 0.6.1 - 2026-04-17

Added more code examples in the documentation.

## 0.6.0 - 2026-04-17

This release loosens the argument types of `sampled_from()` and `one_of()` so callers don't have to pre-materialize a `Vec`.

- `sampled_from()` now accepts anything convertible into `Cow<'_, [T]>`, so `Vec<T>`, `&[T]`, `&Vec<T>`, and `&[T; N]` all work directly. Borrowed slices incur zero allocation up front: the generator keeps a `Cow::Borrowed` and only clones individual elements on draw.
- `one_of()` now accepts any `IntoIterator<Item = BoxedGenerator<'_, T>>` rather than requiring a `Vec`, so callers can pass iterator chains without an intermediate `.collect()`.

Turbofished calls like `sampled_from::<i32>(vec![])` are technically a source-breaking change, since the function now has a second generic parameter, but the non-turbofished call sites (every existing use in practice) continue to work unchanged.

## 0.5.3 - 2026-04-15

This release improves resilience when the hegel server subprocess exits unexpectedly.

- When a server crash is detected, the next call to `hegel()` now transparently starts a fresh server instead of failing with a `PoisonError` or a generic panic.
- Server crash error messages now include the last few lines of `.hegel/server.log` so the root cause is visible without inspecting the log file manually.
- Panics from test functions (including server crashes) are now properly propagated rather than being replaced with a generic "could not find any examples" error.

## 0.5.2 - 2026-04-15

This patch adds an `#[hegel::explicit_test_case]` attribute for providing explicit example-based test cases alongside property-based tests.

## 0.5.1 - 2026-04-13

This release consists entirely of internal refactoring within Hegel to
provide better abstractions over the hegel-core server. It should have
no user visible effect.

## 0.5.0 - 2026-04-12

Fix `vecs(...).unique(true)` not actually enforcing element uniqueness in some cases.

Calling `.unique()` now requires the elements produced by the generator passed to `vecs()` to implement `PartialEq`. This is therefore technically a breaking change, though we expect that the only case where you will need to update your code is when it was previously not working anyway.

## 0.4.6 - 2026-04-10

Bump our pinned hegel-core to [0.4.0](https://github.com/hegeldev/hegel-core/releases/tag/v0.4.0), incorporating the following change:

> This patch changes our CBOR tag for text fields from `6` to `91`, to avoid reserving a "Standards Action" tag, even though it is technically unassigned. See https://www.iana.org/assignments/cbor-tags/cbor-tags.xhtml.
>
> The protocol version is now `0.10`.
>
> — [v0.4.0](https://github.com/hegeldev/hegel-core/releases/tag/v0.4.0)

## 0.4.5 - 2026-04-07

This release adds more configuration parameters to `generators::text()`:

```rust
gs::text().codec("ascii");
gs::text().alphabet("abc");
gs::text().min_codepoint(0x20).max_codepoint(0x7E);
gs::text().categories(&["L", "Nd"]);
gs::text().exclude_categories(&["Cc"]);
gs::text().include_characters("@#$");
gs::text().exclude_characters("\n\t");
```

As well as a new `characters()` generator:

```rust
let c: char = tc.draw(gs::characters());
let c: char = tc.draw(gs::characters().codec("ascii"));
```

## 0.4.4 - 2026-04-07

This patch improves our output for failing test cases. We now print drawn values using variable names from the test function, instead of numbered `Draw` labels:

```rust
#[hegel::test]
fn my_test(tc: hegel::TestCase) {
    let x: i32 = tc.draw(gs::integers());
    let y: i32 = tc.draw(gs::integers());
    for _ in 0..2 {
        let z: i32 = tc.draw(gs::integers());
    }
    panic!("");
}

// Previously:
// Draw 1: 0
// Draw 2: 1
// Draw 3: 0
// Draw 4: 3

// Now:
// let x = 0;
// let y = 1;
// let z_1 = 0;
// let z_2 = 3;
```

## 0.4.3 - 2026-04-03

This patch updates our pinned hegel-core to [0.3.0](https://github.com/hegeldev/hegel-core/releases/tag/v0.3.0), with no user-visible changes.

## 0.4.2 - 2026-04-01

Tests would hang if you were using an old version of hegel-core that didn't support the --stdio flag. This fixes that and adds some comprehensive debugging messages when the server start doesn't work.

## 0.4.1 - 2026-04-01

This patch upgrades [`rand`](https://crates.io/crates/rand) to `0.10` in our `rand` feature.

Thanks to Benjamin Brittain for this patch!

## 0.4.0 - 2026-04-01

This release changes how hegel-core is installed and run:

* Instead of creating a local `.hegel/venv` and pip-installing into it, hegel now uses `uv tool run` to run hegel-core directly. This fixes https://github.com/hegeldev/hegel-rust/issues/108
* If `uv` isn't on your PATH, hegel will automatically download a private copy to `~/.cache/hegel/uv` — so although `uv` is still used under the hood, there's no longer a hard requirement on having uv pre-installed.

## 0.3.7 - 2026-03-30

Add generator for Duration

## 0.3.6 - 2026-03-30

This patch fixes `#[state_machine]` not forwarding attributes on `#[rule]` and `#[invariant]` ([#151](https://github.com/hegeldev/hegel-rust/issues/151)). For example, the following rule is now correctly conditional on the `tokio1` feature:

```rust
#[hegel::state_machine]
impl A {
    #[cfg(feature = "tokio1")]
    #[rule]
    fn f(&mut self, _tc: TestCase) {}
}
```

## 0.3.5 - 2026-03-30

This patch fixes being unable to define `#[hegel::state_machine]` with explicit lifetime or type parameters ([#156](https://github.com/hegeldev/hegel-rust/issues/156)).

## 0.3.4 - 2026-03-27

This patch improves documentation and adds scraped examples to the docs.

## 0.3.3 - 2026-03-27

Fix server crash detection. The client now properly detects when the hegel server process exits unexpectedly, instead of hanging indefinitely.

## 0.3.2 - 2026-03-27

This patch changes the generators import style in our documentation to `use hegel::generators as gs`. We're actively considering the right way to expose these imports to users; you can follow https://github.com/hegeldev/hegel-rust/issues/75 for more.

## 0.3.1 - 2026-03-27

Improve generation and shrinking of `generators::hashsets` and `generators::hashmaps`.

## 0.3.0 - 2026-03-27

This release changes `self` in `#[invariant]` from an immutable reference to a mutable reference:

```rust
# before
#[invariant]
fn my_invariant(&self, ...) {} 

# after
#[invariant]
fn my_invariant(&mut self, ...) {}
```

This will require updating your invariant signatures, but should be strictly more expressive.

## 0.2.6 - 2026-03-26

Bump our pinned hegel-core to [0.2.3](https://github.com/hegeldev/hegel-core/releases/tag/v0.2.3), incorporating the following change:

> This release adds a --stdio flag to hegel-core that allows the calling process to communicate with it directly via stdin and stdout rather than going via a unix socket.
>
> As well as simplifying the interactions with hegel-core, this should enable easier support for Windows later.
>
> — [v0.2.3](https://github.com/hegeldev/hegel-core/releases/tag/v0.2.3)

## 0.2.5 - 2026-03-25

This release extends the tuples! macro to handle 1-tuples and 0-tuples correctly.

## 0.2.4 - 2026-03-25

This release moves over to using the new stdio version of hegel-core.
This should not be a user visible change.

## 0.2.3 - 2026-03-25

This release changes the way the client manages the server to run a single persistent process for the whole test run.

This should improve the performance of running many hegel tests, and also hopefully fixes an intermittent hang we would sometimes see when many hegel tests were run concurrently.

## 0.2.2 - 2026-03-25

This is a no-op release that fixes some publishing problems and has no user-visible changes.

## 0.2.1 - 2026-03-24

This patch improves the documentation for stateful testing.

## 0.2.0 - 2026-03-24

This release makes a bunch of last-minute cleanups to places where our API obviously needed fixing that emerged during docs review.

* Removes `none()` which is a weird Python anachronism
* Makes various places where we had a no-arg method to take a boolean to match `unique(bool)`
* Replaces our various tuplesN functions with a tuples! macro

## 0.1.18 - 2026-03-24

More updates and fixes to documentation.

## 0.1.17 - 2026-03-24

Add comprehensive API documentation, and hide various bits that shouldn't appear in the public docs.

## 0.1.16 - 2026-03-24

Better error message for when `uv` is not found on the PATH.

## 0.1.15 - 2026-03-23

Add `#[hegel::state_machine]` for defining stateful tests.

## 0.1.14 - 2026-03-23

Drop our dependency on the `num` crate.

## 0.1.13 - 2026-03-23

Enable the `#![forbid(future_incompatible)]` and `#![cfg_attr(docsrs, feature(doc_cfg))]` attributes, the latter of which unblocks our docs.rs build.

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
