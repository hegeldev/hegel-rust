//! Ported from hypothesis-python/tests/nocover/test_conjecture_engine.py
//!
//! The Python original tests `ConjectureRunner` / `Shrinker` internals
//! through `run_to_nodes`, `shrinking_from`, and direct calls to
//! `runner.cached_test_function` / `runner.test_function`. Only one of
//! the five tests here (`test_lot_of_dead_nodes`) is expressible at the
//! public `Hegel::new(...).run()` surface; the other four all monkey-
//! patch engine functions or drive Python-specific APIs with no native
//! counterpart.
//!
//! `test_lot_of_dead_nodes` is server-only
//! (`#[cfg(not(feature = "native"))]`): the Python engine finds the
//! unique interesting case `(0, 1, 2, 3)` behind four dead-branch
//! `assume`s via `DataTree`'s exhausted-child tracking, and hegel-rust's
//! native engine currently generates new examples with pure random
//! seeding (`NativeTestCase::new_random(batch_rng)`) without the
//! novel-prefix / exhausted-branch pruning that would let it navigate
//! past `128^4 ≈ 268M` dead paths in 300 calls.
//!
//! Individually-skipped tests:
//!
//! - `test_saves_data_while_shrinking` —
//!   `monkeypatch.setattr(ConjectureRunner, "generate_new_examples",
//!   ...)` to seed a specific `[255]*10` starting buffer and then
//!   asserts `InMemoryExampleDatabase` accumulates every interesting
//!   example seen during shrinking, decoded via `choices_from_bytes`.
//!   hegel-rust's engine has no public surface for injecting an
//!   initial generate_new_examples buffer, and the
//!   `non_covering_examples(db)` + `choices_from_bytes` helpers rely on
//!   Hypothesis's database metakey layout.
//!
//! - `test_can_discard` — same `monkeypatch.setattr(ConjectureRunner,
//!   "generate_new_examples", ...)` pattern to seed the initial buffer
//!   with `n` pairs of byte-choices, then asserts the shrunk node
//!   count is exactly `n` (i.e. the discard pass collapsed duplicates
//!   in-place). No public seed-a-specific-initial-buffer entry in
//!   native.
//!
//! - `test_cached_with_masked_byte_agrees_with_results` — exercises
//!   `runner.cached_test_function([a])`, `runner.cached_test_function([b])`,
//!   `ConjectureData.for_choices([b], observer=runner.tree.new_observer())`,
//!   and `runner.test_function(data_b)`, then compares identity
//!   (`cached_a is cached_b`) against `data_b.nodes` equality. The
//!   native `TargetedRunner::cached_test_function` returns a fresh
//!   `CachedTestResult { status }` each call with no `nodes` field and
//!   no identity semantics, and there is no pluggable observer surface
//!   on `CachedTestFunction`.
//!
//! - `test_node_programs_fail_efficiently` — uses
//!   `shrinker.fixate_shrink_passes([shrinker.node_program("XX")])` and
//!   the `counts_calls(Shrinker.run_node_program)` monkeypatch counter
//!   to assert the node-program pass runs ~255 times on a 256-node
//!   input. The `node_program` deletion pass is absent from the native
//!   shrinker (see `tests/hypothesis/conjecture_shrinker.rs`
//!   docstring), and there is no `max_stall` / `fixate_shrink_passes`
//!   / call-counter surface on `Shrinker`.

#[cfg(not(feature = "native"))]
use std::panic;
#[cfg(not(feature = "native"))]
use std::sync::{Arc, Mutex};

#[cfg(not(feature = "native"))]
use hegel::generators::{self as gs};
#[cfg(not(feature = "native"))]
use hegel::{Hegel, HealthCheck, Settings};

#[cfg(not(feature = "native"))]
#[test]
fn test_lot_of_dead_nodes() {
    // Draw four integers in [0, 127]; the test is only "interesting"
    // when they are exactly (0, 1, 2, 3). Every other path rejects via
    // `assume`, leaving the engine to navigate many dead branches
    // before it finds the match. Mirrors the Python original's
    // `for i in range(4): if data.draw_integer(0, 2**7 - 1) != i:
    // data.mark_invalid()` body.
    let found: Arc<Mutex<Option<[i64; 4]>>> = Arc::new(Mutex::new(None));
    let found_clone = Arc::clone(&found);
    let panic_result = panic::catch_unwind(panic::AssertUnwindSafe(|| {
        Hegel::new(move |tc| {
            let mut out = [0i64; 4];
            for (i, slot) in out.iter_mut().enumerate() {
                let v: i64 = tc.draw(gs::integers::<i64>().min_value(0).max_value(127));
                tc.assume(v == i as i64);
                *slot = v;
            }
            *found_clone.lock().unwrap() = Some(out);
            panic!("DEAD_NODES_FOUND");
        })
        .settings(
            Settings::new()
                .test_cases(300)
                .database(None)
                .derandomize(true)
                .suppress_health_check(HealthCheck::all()),
        )
        .run();
    }));
    if let Err(payload) = panic_result {
        let is_expected = payload
            .downcast_ref::<&str>()
            .map(|s| s.contains("DEAD_NODES_FOUND"))
            .or_else(|| {
                payload
                    .downcast_ref::<String>()
                    .map(|s| s.contains("DEAD_NODES_FOUND"))
            })
            .unwrap_or(false);
        if !is_expected {
            panic::resume_unwind(payload);
        }
    }
    let nodes = found
        .lock()
        .unwrap()
        .take()
        .expect("engine never hit the dead-nodes match");
    assert_eq!(nodes, [0, 1, 2, 3]);
}
