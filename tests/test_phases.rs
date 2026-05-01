//! Ported from hypothesis-python/tests/cover/test_phases.py
//!
//! Tests that depend on server-side phase enforcement (Reuse, Shrink, Generate
//! skipping) require hegel-core with phases support and will be added in a
//! follow-up once that version ships.

mod common;

use hegel::generators as gs;
use hegel::{Phase, Settings, TestCase};

// With phases not including Explicit, explicit cases are skipped.
// The explicit case would fail at runtime (name mismatch: "hello_world" vs "b"),
// but it is never run because Phase::Explicit is not in the phase list.
#[hegel::test(test_cases = 5, phases = [Phase::Reuse, Phase::Generate])]
#[hegel::explicit_test_case(hello_world = "hello world".to_string())]
fn test_does_not_use_explicit_examples(tc: TestCase) {
    let b: bool = tc.draw(gs::booleans());
    let _ = b;
}

// Default phases include Explicit so that explicit_test_case attributes work
// without any phases configuration.
#[test]
fn test_default_phases_include_explicit() {
    let settings = Settings::new();
    assert!(settings.has_phase(Phase::Explicit));
    assert!(settings.has_phase(Phase::Reuse));
    assert!(settings.has_phase(Phase::Generate));
    assert!(settings.has_phase(Phase::Target));
    assert!(settings.has_phase(Phase::Shrink));
}

// When phases are overridden, only the specified phases are active.
#[test]
fn test_overriding_phases_excludes_others() {
    let settings = Settings::new().phases([Phase::Generate]);
    assert!(!settings.has_phase(Phase::Explicit));
    assert!(!settings.has_phase(Phase::Reuse));
    assert!(settings.has_phase(Phase::Generate));
    assert!(!settings.has_phase(Phase::Target));
    assert!(!settings.has_phase(Phase::Shrink));
}
