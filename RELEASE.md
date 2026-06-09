RELEASE_TYPE: minor

This release reworks the stateful-testing value-pool API. The `Variables<T>`
type is renamed to `Pool<T>` and the `variables()` constructor is renamed to
`pool()`. The `draw()` and `consume()` methods are replaced by two generators
you draw from with `tc.draw()`:

- `pool.references()` returns a generator over `&T` — drawing from it yields a
  reference to a value in the pool without removing it (the old `draw()`).
- `pool.values()` returns a generator over `T` — drawing from it removes a value
  from the pool and yields it by value (the old `consume()`).

Routing pool draws through `tc.draw()` means the chosen value is recorded in the
failing-test replay and shrinks like any other draw.

To migrate, rename the type and constructor, and replace draws:

```rust
// Before
use hegel::stateful::{Variables, variables};
let mut accounts: Variables<String> = variables(&tc);
let account = accounts.draw().clone();
let consumed = accounts.consume();

// After
use hegel::stateful::{Pool, pool};
let mut accounts: Pool<String> = pool(&tc);
let account = tc.draw(accounts.references()).clone();
let consumed = tc.draw(accounts.values());
```

The `is_empty()`, `len()`, and `add()` methods are unchanged.
