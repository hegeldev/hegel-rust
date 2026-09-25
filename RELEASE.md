RELEASE_TYPE: minor

This release removes the `PrettyPrintable` trait. Drawn values now print the way `{:?}` shows them: any `Debug` type is printable, and the failing-example report lays that output out through libhegel's new reflower, so a large value still wraps one field or element per line.

`PrettyPrintable` was the protocol a value used to describe itself in Rust-expression syntax (`vec![1, 2]`, `"a".to_string()`, `HashMap::from([…])`). Because of the orphan rule, a test could not implement it for a type from another crate, so drawing a `map` to a `taffy::Style` or a `just(some_foreign_value)` needed a `.print_as_debug()` at every draw site, and a `#[derive(PrettyPrintable)]` type with such a field needed `#[pretty(debug)]` on it. Every one of those annotations is now unnecessary: `map`, `just`, `sampled_from`, `boxed`, `#[hegel::composite]` functions and the `stateful` generators are printable whenever the produced type implements `Debug`, which it almost always already does.

The cost is the Rust-expression syntax. Leaves and collections print in `Debug` form too — `[1, 2]` rather than `vec![1, 2]`, `"a"` rather than `"a".to_string()`, `{1: "a"}` rather than `HashMap::from([(1, "a")])`, `NaN` rather than `f64::NAN`, `2020-02-29` rather than `NaiveDate::from_ymd_opt(2020, 2, 29).unwrap()` — so that a report never mixes the two forms. A derived generator still prints field by field as it draws, in the `Name { field: value }` shape `#[derive(Debug)]` output takes; an enum variant prints as `Variant { field: value }`, without the enum's name, as `Debug` writes it.

Code that named the removed items needs updating:

- `#[derive(hegel::PrettyPrintable)]` and `#[pretty(debug)]`: delete them; `#[derive(Debug)]` is all a type needs.
- `hegel::pretty_print_as_debug!(Type)`: delete it.
- A hand-written `impl PrettyPrintable for Type`: delete it. To print a type differently from its `Debug` output, print through the generator with `.print_with(|value, printer| …)`, which now receives a printer with `debug`, `group`, `seq` and `separator` methods for composing the representation.
- `.print_as_value()`: delete it — value-producing generators print by value already — or, on a hand-written `Generator` of a `Debug` type, replace it with `.print_as_debug()`.
- `hegel::pretty::print_debug_repr(&format!("{value:?}"), printer)`: replace with `printer.debug(&value)`.
- A `PrettyPrintable` bound on a helper's type parameter: replace with `Debug`.

`PrettyPrinter` gains `debug`, which prints any `Debug` value with its bracket structure recovered and laid out by the engine; `reflow`, which does the same for an already-formatted representation; `group` and `seq`, closure-based forms of a delimited group and a comma-separated sequence; and `separator`, the `,` plus break point between two items. Draw-site printability errors now lead with `#[derive(Debug)]` as the fix.
