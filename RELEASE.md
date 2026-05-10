RELEASE_TYPE: minor

This release adds a native Rust backend for test-case generation and shrinking, available behind the `native` feature flag. When enabled, Hegel no longer requires a Python server process -- all generation, shrinking, and database caching happen in-process. This eliminates the server-spawn startup cost and removes Python from the runtime dependency set, making the library easier to deploy.

The native backend is a port of the core Hypothesis engine: random-prefix generation guided by a data tree, per-origin shrinking with the standard pass set, targeting/optimisation via hill-climbing on observed scores, the example database with primary/secondary/pareto sub-keys, and the standard health checks (FilterTooMuch, TooSlow, LargeBaseExample, DataTooLarge). It is activated automatically when the `native` feature is enabled; no code changes are required beyond adding the feature flag.

A few notes on parity:

- **Example database format is intentionally incompatible with Hypothesis's `DirectoryBasedExampleDatabase`.** Path-shape matches but the key hash differs (FNV-1a 64-bit hex vs sha384[:16]), the metakey filename differs (`.hegel-keys` vs `.hypothesis-keys`), and the value-byte encoding is Hegel-specific. Corpora are not portable between the two toolchains. Users who want a shared corpus should layer a translation `ExampleDatabase` via `MultiplexedNativeDatabase`.
- **Some shrink-quality refinements remain in flight.** A small number of well-characterised gaps are tracked in the project's audit notes (e.g. shrink-time probes don't yet hit the LRU cache; `find_integer`-based passes assume a monotone predicate that A21's `shrink_towards`-aware sort_key turned U-shaped). These don't affect correctness — counterexamples still shrink — but Hypothesis may produce minimally simpler outputs in some edge cases.
- **`print_blob` / `reproduce_failure` is not implemented on either backend.** Counterexample replay is via the database, not via copy-pasteable seed strings.

This release also adds `generators::deferred()`, which creates a generator that can be declared before it is defined. This enables forward references, which are needed for defining mutually recursive or self-recursive generators.

```rust
use hegel::generators::{self as gs, Generator};

enum Tree {
    Leaf(i32),
    Branch(Box<Tree>, Box<Tree>),
}

let tree = gs::deferred::<Tree>();
let leaf = gs::integers::<i32>().map(Tree::Leaf);
let branch = hegel::tuples!(tree.generator(), tree.generator())
    .map(|(l, r)| Tree::Branch(Box::new(l), Box::new(r)));
tree.set(hegel::one_of!(leaf, branch));
```

Call `.generator()` to get handles that can be passed to other generators, then call `.set()` to provide the actual implementation. `set` consumes the definition, so it can only be called once.
