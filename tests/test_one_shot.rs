//! Tests for the `one_shot` setting, which runs a single test case in final
//! mode with no shrinking or replay.
//!
//! Requires a `hegel-core` that implements the `one_shot` protocol option.
//! Against older servers, the `one_shot` setting panics with a clear error
//! message; tests that directly verify one-shot semantics skip when the
//! pinned `hegel-core` is too old, and start running automatically once the
//! pinned version is bumped to one that supports the feature.

use hegel::generators as gs;
use std::cell::Cell;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Minimum `hegel-core` version that supports the `one_shot` protocol option.
/// Update this if `one_shot` lands in a different release than currently
/// anticipated.
const ONE_SHOT_MIN_VERSION: (u32, u32, u32) = (0, 4, 5);

fn parse_semver(text: &str) -> Option<(u32, u32, u32)> {
    let start = text.find("version ")?;
    let rest = text[start + "version ".len()..].trim_start();
    let ver = rest.split(')').next()?.trim();
    let parts: Vec<u32> = ver.split('.').filter_map(|p| p.parse().ok()).collect();
    if parts.len() == 3 {
        Some((parts[0], parts[1], parts[2]))
    } else {
        None
    }
}

fn server_version() -> Option<(u32, u32, u32)> {
    if let Ok(cmd) = std::env::var("HEGEL_SERVER_COMMAND") {
        let out = std::process::Command::new(&cmd)
            .arg("--version")
            .output()
            .ok()?;
        parse_semver(&String::from_utf8_lossy(&out.stdout))
    } else {
        parse_semver(&format!(
            "hegel (version {})",
            hegel::pinned_server_version()
        ))
    }
}

fn hegel_supports_one_shot() -> bool {
    server_version().is_some_and(|v| v >= ONE_SHOT_MIN_VERSION)
}

#[test]
fn one_shot_runs_exactly_one_test_case() {
    if !hegel_supports_one_shot() {
        return;
    }
    let count = Cell::new(0);

    hegel::Hegel::new(|tc| {
        let _ = tc.draw(gs::booleans());
        count.set(count.get() + 1);
    })
    .settings(hegel::Settings::new().one_shot(true).test_cases(100))
    .run();

    assert_eq!(count.get(), 1);
}

#[test]
fn one_shot_does_not_shrink_or_replay_on_failure() {
    if !hegel_supports_one_shot() {
        return;
    }
    static COUNT: AtomicUsize = AtomicUsize::new(0);

    let result = std::panic::catch_unwind(|| {
        hegel::Hegel::new(|tc| {
            let _ = tc.draw(gs::integers::<i32>().min_value(0).max_value(1_000_000_000));
            COUNT.fetch_add(1, Ordering::SeqCst);
            panic!("always fails");
        })
        .settings(hegel::Settings::new().one_shot(true))
        .run();
    });

    assert!(result.is_err(), "expected one-shot failure to panic");
    assert_eq!(
        COUNT.load(Ordering::SeqCst),
        1,
        "one_shot must not shrink or replay"
    );
}

#[test]
fn one_shot_runs_in_final_mode_so_note_is_emitted() {
    if !hegel_supports_one_shot() {
        return;
    }
    // In final mode, `note()` writes to stderr. We can't easily capture that
    // from within the test, but we can at least verify that the test runs
    // in final mode by calling `note()` — this exercises the is_last_run
    // branch of TestCase. Coverage of the actual stderr output is handled
    // via the end-to-end output tests.
    hegel::Hegel::new(|tc| {
        let x = tc.draw(gs::integers::<i32>());
        tc.note(&format!("x = {x}"));
    })
    .settings(hegel::Settings::new().one_shot(true))
    .run();
}

#[test]
fn one_shot_false_runs_normally() {
    let count = Cell::new(0);
    hegel::Hegel::new(|tc| {
        let _ = tc.draw(gs::integers::<i32>());
        count.set(count.get() + 1);
    })
    .settings(hegel::Settings::new().one_shot(false).test_cases(5))
    .run();
    assert_eq!(count.get(), 5);
}

/// Verifies that the `#[hegel::test]` attribute macro accepts
/// `one_shot = true`. The actual runtime semantics of one-shot are covered
/// by `one_shot_runs_exactly_one_test_case` above (via the `Settings`
/// builder). This attribute test is ignored at runtime because it cannot
/// pass against a pinned `hegel-core` that is too old — its value is in
/// exercising the macro expansion at compile time.
#[hegel::test(one_shot = true)]
#[ignore = "requires hegel-core with one_shot support; kept for macro expansion coverage"]
fn attribute_form_with_one_shot(tc: hegel::TestCase) {
    let _ = tc.draw(gs::integers::<i32>());
}

#[test]
fn one_shot_panics_against_too_old_server() {
    if hegel_supports_one_shot() {
        return;
    }
    let result = std::panic::catch_unwind(|| {
        hegel::Hegel::new(|tc| {
            let _ = tc.draw(gs::booleans());
        })
        .settings(hegel::Settings::new().one_shot(true))
        .run();
    });
    let err = result.expect_err("expected panic when hegel-core is too old");
    let msg = err
        .downcast_ref::<String>()
        .map(String::as_str)
        .or_else(|| err.downcast_ref::<&str>().copied())
        .unwrap_or("");
    assert!(
        msg.contains("Settings::one_shot requires hegel-core"),
        "unexpected panic message: {msg}"
    );
}

#[test]
fn one_shot_can_use_full_generator_surface() {
    if !hegel_supports_one_shot() {
        return;
    }
    hegel::Hegel::new(|tc| {
        let xs: Vec<i32> = tc.draw(
            gs::vecs(gs::integers::<i32>().min_value(0).max_value(100))
                .min_size(1)
                .max_size(5),
        );
        assert!(!xs.is_empty());
        assert!(xs.len() <= 5);
        for x in xs {
            assert!((0..=100).contains(&x));
        }
    })
    .settings(hegel::Settings::new().one_shot(true))
    .run();
}
