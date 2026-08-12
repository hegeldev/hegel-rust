RELEASE_TYPE: patch

This patch adds parallel test-case execution. The new `Settings::threads(n)` setting (default 1, overridable at runtime with the `HEGEL_THREADS` environment variable) runs test-case bodies on `n` worker threads during the generation phase; shrinking, database replay, and the failure report stay sequential, so every discovered failure still shrinks to a minimal counterexample and replays exactly via its reproduce blob and the failure database. Tests written with `#[hegel::test]` and `#[hegel::main]` support this out of the box:

```rust
#[hegel::test(threads = 4)]
fn my_test(tc: hegel::TestCase) { ... }
```

With `threads` above 1 the search itself is not schedule-reproducible, even with a fixed seed: which test case is generated next depends on the order completions arrive. `#[hegel::standalone_function]` does not yet support `threads` above 1 and fails with a usage error if it is requested.

Code calling the `Hegel` builder directly reaches parallel execution through the new `Hegel::new_concurrent` / `Hegel::run_concurrent` entry points, which require the test function to be `Fn(TestCase) + Sync`; `Hegel::new` / `Hegel::run` are unchanged and single-threaded.
