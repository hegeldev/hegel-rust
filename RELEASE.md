RELEASE_TYPE: minor

This release adds a `one_shot` setting that runs exactly one test case in final
mode, with no shrinking, replay, or other exploration:

```rust
use hegel::generators as gs;

#[hegel::test(one_shot = true)]
fn workload(tc: hegel::TestCase) {
    let xs: Vec<i32> = tc.draw(gs::vecs(gs::integers()).min_size(1));
    // do something with xs
}
```

This is mostly intended for Antithesis workloads and similar environments where
the system under test cannot be reset between iterations and shrinking would be
meaningless — you are effectively using Hegel purely for data generation.

Requires hegel-core 0.4.4 or later (the underlying `one_shot` protocol support
was added in [hegel-core#97](https://github.com/hegeldev/hegel-core/pull/97)).
