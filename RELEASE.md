RELEASE_TYPE: patch

This patch adds `Generator::label`, a provided method giving every generator a label of its own, and the functions `generators::label_from_name` and `generators::combine_labels` for deriving one. A label is an opaque `u64` identifying a generator to the engine, which treats two spans with the same label as coming from the same generator when it shrinks and mutates test cases; it has no other meaning. The default label is derived from the generator's type name, so hand-written generators get a stable label with no extra work; generators built from others should combine a label of their own with their components':

```rust
use hegel::generators::{self as gs, Generator};

impl<T, G: Generator<T>> Generator<(T, T)> for Pairs<G> {
    fn label(&self) -> u64 {
        gs::combine_labels(&[gs::label_from_name("mycrate.pairs"), self.inner.label()])
    }

    fn do_draw(&self, tc: &hegel::TestCase) -> (T, T) {
        (self.inner.do_draw(tc), self.inner.do_draw(tc))
    }
}
```

Previously every collection shared one label, every `map` another and so on, regardless of what they contained. The built-in combinators, `#[derive(DefaultGenerator)]` and `#[composite]` now label their spans this way, so `vecs(integers())` and `vecs(text())` have different labels, which should let the engine's span-swapping shrink passes and mutations line up spans that actually correspond. The hidden `generators::labels` constants and `generators::fnv1a_hash` are gone; the engine's `hegel_label_t` enum they mirrored no longer exists.
