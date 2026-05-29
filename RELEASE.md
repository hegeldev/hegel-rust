RELEASE_TYPE: patch

This patch makes two improvements to the libhegel C bindings.

It adds variable-pool support, closing the last feature-parity gap between the
native C API and the in-process Rust engine. Variable pools are the primitive
behind stateful testing (`stateful::Variables` and `#[hegel::state_machine]`):
the engine tracks a set of opaque variable ids that a test can draw from and
shrink over. The C API now exposes them through three new entry points:

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

It also stops engine panics from leaking Rust implementation detail to the
embedding process's stderr. When the engine aborts a run — for example on a
failed health check like `FilterTooMuch` — it panics on libhegel's internal
worker thread, and Rust's default panic hook printed a line like
`thread 'hegel-worker' panicked at src/native/test_runner.rs:329:21:` before
the panic was caught. libhegel already catches that panic and surfaces a clean
message through `hegel_run_result` / `hegel_failure_*`, so the stderr line was
pure noise to a C consumer. libhegel now installs a panic hook that swallows
the default output for panics on its own worker thread; panics elsewhere are
left untouched.

As part of the pool work, the pool methods on the internal `DataSource` trait
now use `i64` for pool and variable ids instead of `i128`. The previous `i128`
was an artifact of the infallible CBOR-integer conversion and was never needed
— pool ids are small counters, and this matches the `i64` already used for
collection ids. The user-facing `stateful::Variables` API is unchanged.
