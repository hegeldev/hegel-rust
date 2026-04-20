> [!IMPORTANT]
> We're excited you're checking out Hegel! Hegel is in beta, and we'd love for you to try it and [report any feedback](https://github.com/hegeldev/hegel-rust/issues/new).
>
> As part of our beta, we may make breaking changes if it makes Hegel a better property-based testing library. If that instability bothers you, please check back in a few months for a stable release!
>
> See https://hegel.dev/compatibility for more details.

# Hegel for Rust

* [Documentation](https://docs.rs/hegeltest)
* [Website](https://hegel.dev)

`hegel-rust` is a property-based testing library for Rust. `hegel-rust` is based on [Hypothesis](https://github.com/hypothesisworks/hypothesis), using the [Hegel](https://hegel.dev/) protocol.

## Installation

To install: `cargo add --dev hegeltest`.

Hegel will use [uv](https://docs.astral.sh/uv/) to install the required [hegel-core](https://github.com/hegeldev/hegel-core) server component.
If `uv` is already on your path, it will use that, otherwise it will download a private copy of it to ~/.cache/hegel and not put it on your path.
See https://hegel.dev/reference/installation for details.

If you are windows (which is only supported on a somewhat experimental basis right now), the automatic uv installation doesn't work yet, and you will need to [install uv yourself](https://docs.astral.sh/uv/getting-started/installation/#__tabbed_1_2) and make sure it is on your path.

## Quickstart

Here's a quick example of how to write a Hegel test:

```rust
use hegel::generators as gs;
use hegel::TestCase;

fn my_sort(ls: &[i32]) -> Vec<i32> {                                                                 
    let mut result: Vec<i32> = ls.to_vec(); 
    result.sort();                         
    result.dedup();
    result                                                                                           
}

#[hegel::test]
fn test_matches_builtin(tc: TestCase) {
    let mut vec1 = tc.draw(gs::vecs(gs::integers::<i32>()));
    let vec2 = my_sort(&vec1);
    vec1.sort();
    assert_eq!(vec1, vec2);
}
```

This test will fail when run with `cargo test`! Hegel will produce a minimal failing test case for us:

```
Draw 1: [0, 0]
thread 'test_matches_builtin' (2) panicked at src/main.rs:15:5:
assertion `left == right` failed
  left: [0, 0]
 right: [0]
```

Hegel reports the minimal example showing that our sort is incorrectly dropping duplicates. If we remove `result.dedup()` from `my_sort()`, this test will then pass (because it's just comparing the standard sort against itself). 
