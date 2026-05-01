RELEASE_TYPE: minor

This release adds a native Rust backend for test-case generation and shrinking, available behind the `native` feature flag. When enabled, Hegel no longer requires a Python server process -- all generation, shrinking, and database caching happen in-process. This should significantly improve startup latency and make the library easier to deploy.

The native backend is a port of the core Hypothesis engine and supports the same generation and shrinking semantics. It is activated automatically when the `native` feature is enabled; no code changes are required beyond adding the feature flag.

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
