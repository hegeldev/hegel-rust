RELEASE_TYPE: minor

This release changes `self` in `#[invariant]` from an immutable reference to a mutable reference:

```rust
# before
#[invariant]
fn my_invariant(&self, ...) {} 

# after
#[invariant]
fn my_invariant(&mut self, ...) {}
```

This will require updating your invariant signatures, but should be strictly more expressive.
