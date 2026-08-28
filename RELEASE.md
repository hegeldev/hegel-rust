RELEASE_TYPE: patch

This patch adds two new generators for rearranging a fixed list of values: `gs::permutations()`, which generates the elements in a randomly chosen order, and `gs::subsequences()`, which generates subsets of the elements in their original order.

```rust
use hegel::generators as gs;

#[hegel::test]
fn my_test(tc: hegel::TestCase) {
    let perm: Vec<i32> = tc.draw(gs::permutations(vec![1, 2, 3, 4, 5]));
    let sub: Vec<i32> = tc.draw(gs::subsequences(vec![1, 2, 3, 4, 5]));
}
```

Both support `min_size` and `max_size` bounds. On `subsequences` these constrain how many elements are included; setting them on `permutations` generates an ordered sample without replacement — a permutation of a subset of the elements — instead of reordering all of them. Permutations shrink towards the original order, and subsequences towards fewer elements taken from earlier in the list.
