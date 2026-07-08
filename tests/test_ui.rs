//! Compile-diagnostic UI tests: every case in `tests/ui/` is a program that
//! must FAIL to compile, with diagnostics matching its checked-in `.stderr`
//! golden file. These pin the compile-time error messages of the hegel
//! macros (and a couple of deliberate type-level properties).
//!
//! To (re)generate the goldens after intentionally changing a diagnostic:
//! `TRYBUILD=overwrite cargo test --test test_ui`.

use std::process::{Command, Stdio};

/// rustc changed the E0283 ambiguity note from ``cannot satisfy `_: Debug` ``
/// to ``the type must implement `Debug` `` somewhere after the MSRV
/// toolchain, so the one case whose golden contains that note keeps one
/// golden per wording (same source, same assertion). Probe the active
/// toolchain's actual wording with a dependency-free snippet rather than
/// maintaining a version table.
fn e0283_note_uses_must_implement_wording() -> bool {
    let dir = tempfile::tempdir().unwrap();
    let probe = dir.path().join("probe.rs");
    std::fs::write(
        &probe,
        "fn foo<T: std::fmt::Debug>() -> T { unimplemented!() }\n\
         fn main() { let _ = foo(); }\n",
    )
    .unwrap();
    let rustc = std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into());
    let output = Command::new(rustc)
        .args(["--edition", "2021", "--crate-name", "probe"])
        .arg(&probe)
        .current_dir(dir.path())
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert!(
        !output.status.success(),
        "the E0283 probe unexpectedly compiled"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    if stderr.contains("the type must implement") {
        return true;
    }
    if stderr.contains("cannot satisfy") {
        return false;
    }
    // A third wording: fail loudly so a matching golden set gets added
    // instead of the mismatch surfacing as an opaque trybuild diff.
    panic!("unrecognized E0283 note wording; add a golden set for it:\n{stderr}");
}

#[test]
fn ui() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/ui/*.rs");
    // The E0283 diagnostic also enumerates `PrintableGenerator` implementors,
    // and the feature-gated extras add entries to that list, so the golden is
    // split by feature set as well: CI runs the suite with default features
    // and with `--all-features`, and each gets its own golden per wording.
    let all_features = cfg!(all(feature = "jiff", feature = "chrono"));
    let golden = match (e0283_note_uses_must_implement_wording(), all_features) {
        (true, false) => "tests/ui-e0283-current/*.rs",
        (false, false) => "tests/ui-e0283-msrv/*.rs",
        (true, true) => "tests/ui-e0283-current-all-features/*.rs",
        (false, true) => "tests/ui-e0283-msrv-all-features/*.rs",
    };
    t.compile_fail(golden);
}
