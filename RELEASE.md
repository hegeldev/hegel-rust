RELEASE_TYPE: minor

This release makes `generators::recursive()` print compositionally: the result is a `PrintableGenerator` exactly when the leaf generator and the generator the branch function returns both are, and drawn values print with those generators' own representations. Previously the result was printable whenever the produced type implemented `PrettyPrintable`, printing by value and ignoring how the component generators print. `SubtreeGenerator` is now a `PrintableGenerator` too, so branch functions can pass subtrees to printable combinators and draw them with `tc.draw(..)`.

Migration notes:

- `RecursiveGenerator` now carries its component generator types (`RecursiveGenerator<T, G, F, R>`), which include closure types, so helper functions can no longer name it as a return type: return `impl Generator<T>` (or `impl PrintableGenerator<T>`) instead, applying `max_depth`/`max_leaves` before returning.
- A recursive generator whose leaf or branch generator is not printable no longer satisfies `tc.draw(..)` even when the produced type implements `PrettyPrintable`; make the component printable (`.print_as_value()`, `.print_as_debug()`, or `.print_with(..)`) or draw with `tc.draw_silent(..)`.
