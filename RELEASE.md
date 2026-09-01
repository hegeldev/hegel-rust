RELEASE_TYPE: patch

This patch adds three new generators that draw from a fixed list of values: `gs::permutations()` generates all of the elements in a randomly chosen order, `gs::subsequences()` generates subsets of the elements in their original order, and `gs::samples()` generates samples of the elements — with replacement by default, or without replacement via `without_replacement()`.

```rust
use hegel::generators as gs;

#[hegel::test]
fn my_test(tc: hegel::TestCase) {
    let perm: Vec<i32> = tc.draw(gs::permutations(vec![1, 2, 3, 4, 5]));
    let sub: Vec<i32> = tc.draw(gs::subsequences(vec![1, 2, 3, 4, 5]));
    let sample: Vec<i32> = tc.draw(gs::samples(vec![1, 2, 3, 4, 5]).max_size(10));
}
```

`subsequences` and `samples` support `min_size` and `max_size` bounds on how many elements are included. Permutations shrink towards the original order; subsequences and samples shrink towards fewer elements, taken from earlier in the list.
