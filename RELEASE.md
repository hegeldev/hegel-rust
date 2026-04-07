RELEASE_TYPE: patch

This patch improves our output for failing test cases. We now print drawn values using variable names from the test function, instead of numbered `Draw` labels:

```rust
#[hegel::test]
fn my_test(tc: hegel::TestCase) {
    let x: i32 = tc.draw(gs::integers());
    let y: i32 = tc.draw(gs::integers());
    for _ in 0..2 {
        let z: i32 = tc.draw(gs::integers());
    }
    panic!("");
}

// Previously:
// Draw 1: 0
// Draw 2: 1
// Draw 3: 0
// Draw 4: 3

// Now:
// let x = 0;
// let y = 1;
// let z_1 = 0;
// let z_2 = 3;
```

Additionally, adds an `#[hegel::explicit_test_case]` attribute for providing explicit example-based test cases alongside property-based tests.
