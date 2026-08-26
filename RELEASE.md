RELEASE_TYPE: patch

This patch adds `generators::recursive`, a combinator for generating recursively defined data such as trees or JSON documents. It takes a generator for the leaf values and a function that builds one level of branch structure from a generator of subtrees:

```rust
let json = gs::recursive(
    gs::floats::<f64>().map(Json::Number),
    |json| gs::vecs(json).max_size(5).map(Json::Array),
);
```

Generated sizes cover everything the caps allow, from a single leaf up to the limits set by the `max_depth` and `max_leaves` builder methods; a generation attempt that outgrows `max_leaves` is discarded and retried with a lower branching probability.
