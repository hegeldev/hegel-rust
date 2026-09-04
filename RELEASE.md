RELEASE_TYPE: patch

This patch improves the ergonomics of draw-time printing:

- `BoxedGenerator<T>` is now a `PrintableGenerator` whenever `T` implements `PrettyPrintable`, printing drawn values by their own representation. A `.boxed()` in a generator definition no longer forces printing annotations onto every downstream draw site. `.boxed_printable()` remains the way to keep a custom printing strategy through the erasure.
- `PrettyPrintable` is implemented for more standard-library types: the range types and `Bound`, `VecDeque`, `LinkedList`, `BinaryHeap`, the `NonZero` integers, `Cow`, and `Path`/`PathBuf`. The `chrono`, `jiff`, and `serde_json` integrations add impls for `Month`, `Days`, `Months`, `IsoWeek`, `TimeZone`, `AmbiguousOffset`, and `Map<String, Value>`. Every generator `gs::default()` returns can now be passed to `tc.draw` (several, such as `PathBuf`'s, could previously only be drawn silently).
- Generators defined with `derive_generator!` implement `PrintableGenerator` whenever every field type is `PrettyPrintable`, printing `Name { field: value }` expressions.
- The new `TestCase::draw_named` reports a draw under an explicit name. Use it in helper functions, whose draws otherwise print as the anonymous `draw_1`, `draw_2`, ….
- The new `hegel::prelude` module exports the traits and entry points most tests need, so one `use hegel::prelude::*;` covers them.
- `#[derive(PrettyPrintable)]` on a type with a non-printable field now reports an error pointing at that field, stating that every field must be `PrettyPrintable` and suggesting `#[pretty(debug)]`, instead of draw-site advice attached to the derive. Draw-site printability errors now lead with the once-per-type fix (implementing `PrettyPrintable`) and explain how `-> impl Generator<..>` return types and `.boxed()` interact with printability.
- The `hegel::pretty` module docs now explain the whole printing system: what is printable out of the box, how to make your own types printable, the escape hatches for foreign types, type erasure, and draw naming.
