RELEASE_TYPE: minor

This release fixes a number of bugs found in a full review of the frontend, generators, and proc macros, alongside the engine fixes in the corresponding `hegeltest-c` release (whose regex, shrinking, and replay fixes all surface here — see its changelog).

Breaking changes:

- `Settings::suppress_health_check` now *replaces* any previously configured suppressions, like `Settings::phases`, instead of accumulating across calls. Callers that chained multiple calls to build up a set should pass all the checks in one call.
- Several generator configurations that previously misbehaved silently are now clean usage errors: `gs::hashsets(...)`/`gs::hashmaps(...)` with a `min_size` larger than the element generator's distinct-value pool (previously returned a too-small collection, violating the documented contract), `gs::durations().min_value(...)` beyond `u64::MAX` nanoseconds (previously generated values below the requested minimum), `gs::uuids().version(n)` outside 1–5 (previously generated non-RFC-4122 output), and chrono time/datetime bounds using a mid-day leap-second representation (previously could generate values outside the bounds).
- The write-only `panic_message` and `reproduce_blob` fields have been removed from the doc-hidden `backend::Failure` type.

Bug fixes:

- Builder methods called on a string-shaped generator (`text`, `characters`, `from_regex`, `domains`) after it had already been drawn from were silently ignored; they now take effect.
- Running out of data in the middle of a `#[rule]` now unwinds through `hegel::stateful::run()` as an overrun instead of returning normally with a half-applied rule, so code after `run()` can no longer observe torn state. Engine errors from `Pool` operations are classified properly instead of all being treated as out-of-data.
- A test body that caught its own panic no longer donates that panic's location to a later failure on the same thread, which could group failures under the wrong origin.
- In `Mode::SingleTestCase`, a failed `assume()` inside `tc.repeat(...)` skips that iteration and continues, matching normal mode, instead of silently ending the supposedly endless loop.
- `hegel::with_output_override` restores the previous sink even if the wrapped closure panics, and explicit-test-case replay output goes through the output sink like every other replay line.
- `#[derive(DefaultGenerator)]` reports clean compile errors for generic types, zero-variant enums, and fields named `new` or `boxed` (previously a proc-macro panic or a confusing error pointing into generated code), and an enum with variants `Foo(...)` and `FooWith { ... }` now compiles. Generated code is fully qualified, so a local `mod hegel` or a shadowed `Vec` no longer breaks expansion, and a doc comment on a `#[rule]` no longer emits an `unused_doc_comments` warning into your crate.
- `#[hegel::composite]` and `#[hegel::state_machine]` now reject arguments instead of silently ignoring them, and `#[hegel::test]` rejects a declared return type with a targeted message.

Improvements:

- `gs::hashmaps` with an enumerable key generator (e.g. `sampled_from`) draws keys without replacement, so maps that must contain most of a small key alphabet generate efficiently instead of tripping the `TooSlow` health check.
- `.filter(...)` on an enumerable generator computes the filtered value set once instead of re-cloning the source's elements on every draw, and `one_of` generators of enumerable children are themselves enumerable.
- `hegel::extras::serde_json::values()` bounds its recursive arrays and objects so generated JSON trees terminate naturally instead of routinely exhausting the choice buffer.
- `HashSet<T>` now has a `DefaultGenerator` impl, matching `Vec` and `HashMap`, and `hegel::extras::chrono::naive_weeks()` keeps its default range clear of `NaiveDate::MIN`/`MAX`, where chrono's own `NaiveWeek` accessors panic.
- `text()` and `binary()` with `min_size > 100` and no `max_size` generate lengths in `[min_size, min_size + 100]` instead of collapsing to a fixed length.
