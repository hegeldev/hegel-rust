RELEASE_TYPE: patch

This patch adds `hegel::prelude`, which brings the items most tests need into scope in one import ([#75](https://github.com/hegeldev/hegel-rust/issues/75)):

```rust
use hegel::prelude::*;

#[composite]
fn even_integers(tc: &TestCase) -> i32 {
    tc.draw(gs::integers::<i32>().map(|n: i32| n.wrapping_mul(2)))
}

#[hegel::test]
fn test_sum_of_even_integers_is_even(tc: TestCase) {
    let xs = tc.draw(gs::vecs(even_integers()));
    let total = xs.iter().fold(0i32, |acc, n| acc.wrapping_add(*n));
    assert_eq!(total % 2, 0);
}
```

The prelude re-exports `TestCase`, the `Generator` trait, `#[composite]`, and the `generators` module under both its full name and the conventional `gs` alias. The `Generator` import is important: without it in scope, `map` and `filter` resolve against `Iterator` and rustc reports that your generator "is not an iterator". If you bind `gs` to something of your own, your explicit import shadows the prelude's without an ambiguity error.

It omits `hegel::test`, whose name would collide with the standard `#[test]` attribute, so keep writing `#[hegel::test]` on your test functions.
