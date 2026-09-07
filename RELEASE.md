RELEASE_TYPE: minor

This release adds `#[invariant(always_run)]` for stateful tests ([#449](https://github.com/hegeldev/hegel-rust/issues/449)). A plain `#[invariant]` is checked in full on the machine's initial and final state and sampled in between; an always-run invariant runs after every rule (at every join point, for concurrent machines) instead. Use it for invariants that must observe every intermediate state, including invariants that mutate state when checked:

```rust
#[invariant(always_run)]
fn no_unobserved_writes(&mut self, _: TestCase) {
    assert!(self.writes_since_last_check <= 1);
    self.writes_since_last_check = 0;
}
```

For hand-written `StateMachine` implementations this is a breaking change: `invariants()` now returns `Vec<Invariant<Self>>` instead of `Vec<Rule<Self>>` — construct entries with `Invariant::new` (sampled) or `Invariant::new_always_run`. `ConcurrentInvariant` gains the same `always_run` field and `new_always_run` constructor.
