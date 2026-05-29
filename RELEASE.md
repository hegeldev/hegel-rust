RELEASE_TYPE: minor

This release adds variable-pool support to the libhegel C bindings, closing the
last feature-parity gap between the native C API and the in-process Rust engine.

Variable pools are the primitive behind stateful testing (`stateful::Variables`
and `#[hegel::state_machine]`): the engine tracks a set of opaque variable ids
that a test can draw from and shrink over. The C API now exposes them through
three new entry points:

```c
int64_t pool_id;
hegel_new_pool(tc, &pool_id);

int64_t var_id;
hegel_pool_add(tc, pool_id, &var_id);          // register a generated value

int64_t drawn;
hegel_pool_generate(tc, pool_id, false, &drawn); // reuse one; consume=true removes it
```

A caller keeps its own map from variable id to the value it generated, exactly
as `Variables<T>` holds a `HashMap`. `hegel_pool_generate` returns
`HEGEL_E_STOP_TEST` when the pool has no active variables.

As part of this, the pool methods on the public `backend::DataSource` trait
(`new_pool`, `pool_add`, `pool_generate`) now use `i64` for pool and variable
ids instead of `i128`. The previous `i128` was an artifact of the infallible
CBOR-integer conversion and was never needed — pool ids are small counters, and
this matches the `i64` already used for collection ids. Code that implements or
calls `DataSource` directly with these methods will need to update its id types;
the user-facing `stateful::Variables` API is unchanged.
