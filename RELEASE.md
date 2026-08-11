RELEASE_TYPE: minor

This release replaces `Debug`-based reporting of drawn values with a
pretty-printing system. Failing examples now print each drawn value as a
valid Rust expression — `vec![1, 2]`, `HashMap::from([(1, false)])`,
`Some("x".to_string())`, `Ipv4Addr::new(10, 0, 0, 1)` — laid out and
wrapped like source code, so a reported counterexample can be pasted
straight back into a test. Values print as they are drawn, which is what
lets rejected attempts (filter retries, duplicate collection elements)
disappear from the report and lets a value whose representation is only
known during test execution — a Hegel-controlled RNG, say — fill in its
output as it is used. Notes made mid-draw (from inside a composite body)
now flush after the enclosing draw's line instead of splicing into it, and
draws made from cloned `TestCase`s on other threads appear at the point
where the clone was made, deterministically, regardless of how the threads
interleave.

The printing protocol is two new traits and a document type. A value
describes its own representation through `PrettyPrintable` (implemented
for the standard types the generator library produces, derivable with
`#[derive(PrettyPrintable)]`, and available for any `Debug` type through
`pretty_print_as_debug!` or `pretty::print_debug_repr`); a generator that
can print what it draws implements `PrintableGenerator`, which every
built-in generator does — structural combinators (collections, tuples,
`optional`, `one_of!`, `flat_map`) whenever their components are printable,
value combinators (`map`, `filter`, composites) whenever the produced type
is `PrettyPrintable`. `Document` and `PrettyPrinter` expose the layout
engine directly for custom representations.

The headline breaking change: [`TestCase::draw`] now takes an
`impl PrintableGenerator<T>` instead of any generator of a `Debug` type.
Most call sites compile unchanged. A hand-written `Generator`
implementation passed to `tc.draw` needs one of the escape hatches —
`.print_as_value()` prints the value's own `PrettyPrintable`
representation, `.print_as_debug()` prints any `Debug` type,
`.print_with(f)` prints a custom representation (and can mask secrets) —
or can switch to `tc.draw_silent`, which accepts any `Generator` and skips
reporting. To make a hand-written generator printable itself, implement
`PrintableGenerator::do_draw_and_print` and define `do_draw` as
`self.do_draw_and_print(tc, &mut PrettyPrinter::noop())`, so both draw
paths share one body and consume identical choices by construction.

Other breaking changes, all small:

- `#[derive(DefaultGenerator)]` now generates printable generators that
  print field by field, in the same layout `#[derive(PrettyPrintable)]`
  produces. The generated builder methods accept any printable generator
  and are type-changing; a plain `Generator` argument needs one of the
  printing escape hatches above.
- `one_of!` expands to per-arity generator types (`OneOf1Generator` ..
  `OneOf12Generator`, mirroring `tuples!`), printable exactly when every
  arm is. More than 12 arms is now a compile error pointing at the
  vec-based `one_of()`.
- `HegelRandom` is now an opaque struct rather than a public enum. It
  reports the random values the test actually consumed
  (`HegelRandom { consumed: [..] }`) or the seed in true-random mode.
- The settings enums (`HealthCheck`, `Phase`, `Mode`, `Backend`,
  `Verbosity`) are `#[non_exhaustive]`; matches on them need a wildcard
  arm.
- The `Generator::enumerate_values` fast path is gone. Filtered draws and
  unique collections always rejection-sample; a set or map whose
  `min_size` exceeds what its element or key generator can produce now
  surfaces as a `FilterTooMuch` health check at run time instead of an
  eager argument error.
