//! Ported from hypothesis-python/tests/conjecture/test_optimiser.py
//!
//! Every test in the upstream file exercises Hypothesis's targeted
//! property-based testing machinery: `ConjectureData.target_observations`,
//! `ConjectureRunner.best_observed_targets`, and the
//! `ConjectureRunner.optimise_targets()` hill-climbing pass. None of these
//! exist on hegel-rust's native engine — `NativeTestCase` has no
//! `target_observations` surface, `CachedTestFunction` has no
//! `best_observed_targets` tracker, and `src/native/` has no optimiser
//! pass. The upstream pbtkit equivalent (`test_targeting.py`) is
//! whole-file-skipped for the same reason.
//!
//! Individually-skipped tests (blocked on native targeting / optimiser —
//! see TODO.yaml "Implement native targeting/optimiser"):
//!
//! - `test_optimises_to_maximum` — asserts `best_observed_targets["m"] ==
//!   255` after `optimise_targets()`.
//! - `test_optimises_multiple_targets` — asserts three separate
//!   `best_observed_targets` keys are each driven to their max.
//! - `test_optimises_when_last_element_is_empty` — same with a trailing
//!   empty span.
//! - `test_can_optimise_last_with_following_empty` — asserts the
//!   optimiser reaches 255 even when the target is the final non-empty
//!   draw.
//! - `test_can_find_endpoints_of_a_range` (6 parametrize rows: `lower` ×
//!   `upper` × `score_up`) — asserts the optimiser converges on the
//!   lower or upper endpoint of a filtered range.
//! - `test_targeting_can_drive_length_very_high` — asserts the
//!   optimiser drives a bounded-loop length to its cap.
//! - `test_optimiser_when_test_grows_buffer_to_invalid` — asserts the
//!   optimiser preserves target observations across invalid growth.
//! - `test_can_patch_up_examples` — asserts the optimiser patches the
//!   trailing choices after a scored prefix changes.
//! - `test_optimiser_when_test_grows_buffer_to_overflow` — same under
//!   `buffer_size_limit(2)`.
//! - `test_optimising_all_nodes` — `@given(nodes())` run of the
//!   optimiser against every choice kind.

#![cfg(feature = "native")]
