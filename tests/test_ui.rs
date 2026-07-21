//! Compile-diagnostic UI tests: every case in `tests/ui/` is a program that
//! must FAIL to compile, with diagnostics matching its checked-in `.stderr`
//! golden file. These pin the compile-time error messages of the hegel
//! macros (and a couple of deliberate type-level properties).
//!
//! To (re)generate the goldens after intentionally changing a diagnostic:
//! `TRYBUILD=overwrite cargo test --test test_ui`.
//!
//! The `tests/ui-e0283/` case is checked by hand (see [`e0283_diagnostic`])
//! rather than through trybuild: its diagnostic enumerates 8 of the crate's
//! `PrintableGenerator` implementors, and both the entries shown and their
//! count vary with the enabled feature set and — in ways that resist
//! prediction from the version number alone — the exact toolchain. The
//! hand-rolled comparison normalizes that block away and pins everything
//! else exactly.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// rustc changed the E0283 ambiguity note from ``cannot satisfy `_: Debug` ``
/// to ``the type must implement `Debug` `` somewhere after the MSRV
/// toolchain, so the case whose golden contains that note keeps one golden
/// per wording (same source, same assertion). Probe the active toolchain's
/// actual wording with a dependency-free snippet rather than maintaining a
/// version table.
fn e0283_note_uses_must_implement_wording() -> bool {
    let dir = tempfile::tempdir().unwrap();
    let probe = dir.path().join("probe.rs");
    std::fs::write(
        &probe,
        "fn foo<T: std::fmt::Debug>() -> T { unimplemented!() }\n\
         fn main() { let _ = foo(); }\n",
    )
    .unwrap();
    let output = Command::new(rustc_binary())
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
    // A third wording: fail loudly so a matching golden gets added instead
    // of the mismatch surfacing as an opaque diff.
    panic!("unrecognized E0283 note wording; add a golden for it:\n{stderr}");
}

/// rustc also changed how it annotates a "required for" note that points at
/// a `#[derive(..)]` span: the MSRV toolchain says ``unsatisfied trait bound
/// introduced in this `derive` macro`` where newer toolchains say ``type
/// parameter would need to implement …`` and add a "consider manually
/// implementing" help. Probed like
/// [`e0283_note_uses_must_implement_wording`], with a dependency-free
/// derive whose generated impl has an unsatisfiable bound.
fn derive_bound_note_uses_type_parameter_wording() -> bool {
    let dir = tempfile::tempdir().unwrap();
    let probe = dir.path().join("probe.rs");
    std::fs::write(
        &probe,
        "#[derive(Clone)] struct Foo<T>(T);\n\
         struct NoClone;\n\
         fn need<T: Clone>(_: T) {}\n\
         fn main() { need(Foo(NoClone)); }\n",
    )
    .unwrap();
    let output = Command::new(rustc_binary())
        .args(["--edition", "2021", "--crate-name", "probe"])
        .arg(&probe)
        .current_dir(dir.path())
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert!(
        !output.status.success(),
        "the derive-bound probe unexpectedly compiled"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    if stderr.contains("type parameter would need to implement") {
        return true;
    }
    if stderr.contains("unsatisfied trait bound introduced in this") {
        return false;
    }
    panic!("unrecognized derive-bound note wording; add a golden for it:\n{stderr}");
}

fn rustc_binary() -> std::ffi::OsString {
    std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into())
}

/// The `target/<profile>/deps` directory this test binary was built into,
/// which also holds the `libhegel-<hash>.rlib` the case must compile
/// against.
fn deps_dir() -> PathBuf {
    let exe = std::env::current_exe().unwrap();
    exe.parent().unwrap().to_path_buf()
}

/// The most recently built `libhegel` rlib in `deps`: stale rlibs from
/// earlier builds (other feature sets, older sources) can sit alongside it,
/// and the one cargo built or refreshed for this test run is the newest.
fn newest_hegel_rlib(deps: &Path) -> PathBuf {
    std::fs::read_dir(deps)
        .unwrap()
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|path| {
            path.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("libhegel-") && n.ends_with(".rlib"))
        })
        .max_by_key(|path| std::fs::metadata(path).unwrap().modified().unwrap())
        .unwrap_or_else(|| panic!("no libhegel rlib found in {}", deps.display()))
}

/// Normalize the raw rustc stderr for the E0283 case down to its stable
/// content:
///
/// - the `PrintableGenerator` implementors list keeps only its `= help:`
///   header — the entries shown and the "and N others" count vary by
///   feature set and toolchain;
/// - gutter line numbers become `LL` and gutter indentation collapses, so
///   the golden doesn't churn when `src/test_case.rs` (whose `draw` the
///   diagnostic quotes) shifts;
/// - `--> ` pointers into crate sources drop their line:column for the same
///   reason (the case file's own pointer, whose position we control, keeps
///   its position);
/// - rustc's trailing notes about the full type name written to a temp file
///   (a random path), the `--verbose` hint, the "aborting due to" line, and
///   the `--explain` hint carry no information about hegel and are dropped.
fn normalize_e0283_stderr(raw: &str) -> String {
    let mut out = Vec::new();
    let mut in_impl_list = false;
    for line in raw.lines() {
        let trimmed = line.trim_start();
        if in_impl_list {
            // List entries vary by toolchain: backticked "`X` implements
            // `Y`" lines, bare type names, and the "and N others" tail.
            // Everything until the next diagnostic marker is part of the
            // list.
            let starts_marker = trimmed.starts_with('=')
                || trimmed.starts_with('|')
                || trimmed.starts_with("--> ")
                || trimmed.starts_with("note")
                || trimmed.starts_with("help")
                || trimmed.starts_with("error")
                || trimmed.chars().next().is_some_and(|c| c.is_ascii_digit());
            if !starts_marker {
                continue;
            }
            in_impl_list = false;
        }
        if trimmed.starts_with("= help: the following types implement trait")
            || trimmed.starts_with("= help: the following other types implement trait")
        {
            in_impl_list = true;
            out.push(format!(" {trimmed}"));
            continue;
        }
        if trimmed.starts_with("= note: the full name for the type has been written")
            || trimmed.starts_with("= note: consider using `--verbose`")
            || trimmed.starts_with("error: aborting due to")
            || trimmed.starts_with("For more information about this error")
        {
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("--> ") {
            // Some toolchains print crate-source pointers as absolute paths,
            // and Windows uses backslashes; reduce both to the same
            // manifest-relative forward-slash form.
            let rest = rest.replace('\\', "/");
            let manifest = env!("CARGO_MANIFEST_DIR").replace('\\', "/");
            let rest = rest
                .strip_prefix(&manifest)
                .map(|stripped| stripped.trim_start_matches('/'))
                .unwrap_or(&rest);
            let location = if rest.starts_with("tests/") {
                rest.to_string()
            } else {
                rest.rsplitn(3, ':').last().unwrap().to_string()
            };
            out.push(format!(" --> {location}"));
            continue;
        }
        if trimmed.starts_with('|') || trimmed.starts_with('=') {
            out.push(format!(" {trimmed}"));
            continue;
        }
        let digits = trimmed.chars().take_while(|c| c.is_ascii_digit()).count();
        if digits > 0 && trimmed[digits..].trim_start().starts_with('|') {
            let rest = trimmed[digits..].trim_start();
            out.push(format!("LL {rest}"));
            continue;
        }
        out.push(line.to_string());
    }
    while out.last().is_some_and(|line| line.is_empty()) {
        out.pop();
    }
    out.join("\n") + "\n"
}

/// Compile `case` against the freshly built hegel rlib and return its
/// normalized stderr (see [`normalize_e0283_stderr`]). The case must fail to
/// compile.
fn compile_failing_case(case: &str) -> String {
    let deps = deps_dir();
    let rlib = newest_hegel_rlib(&deps);
    let out_dir = tempfile::tempdir().unwrap();
    let output = Command::new(rustc_binary())
        .args(["--edition", "2021", "--emit=metadata", "--color=never"])
        .arg("--extern")
        .arg({
            let mut arg = std::ffi::OsString::from("hegel=");
            arg.push(&rlib);
            arg
        })
        .arg("-L")
        .arg({
            let mut arg = std::ffi::OsString::from("dependency=");
            arg.push(&deps);
            arg
        })
        .arg(case)
        .arg("-o")
        .arg(out_dir.path().join("case.rmeta"))
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert!(
        !output.status.success(),
        "{case} unexpectedly compiled against {}",
        rlib.display()
    );
    normalize_e0283_stderr(&String::from_utf8_lossy(&output.stderr))
}

/// Compare a hand-checked case's normalized diagnostic against its golden,
/// or rewrite the golden under `TRYBUILD=overwrite`.
fn check_against_golden(actual: &str, golden: &str) {
    if std::env::var_os("TRYBUILD").is_some_and(|v| v == "overwrite") {
        std::fs::write(golden, actual).unwrap();
        return;
    }
    let expected = std::fs::read_to_string(golden)
        .unwrap_or_else(|_| panic!("missing golden {golden}; regenerate with TRYBUILD=overwrite"));
    assert_eq!(
        actual, expected,
        "normalized diagnostic does not match {golden}; \
         if the new output is intended, regenerate with TRYBUILD=overwrite"
    );
}

/// The `tests/ui-e0283/` case, checked by hand: its diagnostic enumerates
/// implementors and splits by the active toolchain's E0283 wording (see the
/// module docs). Regenerate with `TRYBUILD=overwrite`, once on a `cannot
/// satisfy` toolchain (MSRV or current stable) and once on a `must
/// implement` one (nightly).
#[test]
fn e0283_diagnostic() {
    let actual = compile_failing_case("tests/ui-e0283/default_cant_infer_through_draw.rs");
    let golden = if e0283_note_uses_must_implement_wording() {
        "tests/ui-e0283/expected-current.stderr"
    } else {
        "tests/ui-e0283/expected-msrv.stderr"
    };
    check_against_golden(&actual, golden);
}

/// The error a user sees when a derived generator's customized field
/// generator is not printable and the result is drawn with `tc.draw`. Pinned
/// because the "required for" chain names the derive's hidden generator
/// type: the headline message and escape-hatch notes have to carry the
/// explanation on their own. Checked by hand for the same reason as the
/// E0283 case — the diagnostic enumerates `PrintableGenerator` implementors,
/// which vary with the feature set — and golden-split by the derive-bound
/// note wording. Regenerate with `TRYBUILD=overwrite`, once on the MSRV
/// toolchain and once on a current one.
#[test]
fn derived_generator_non_printable_field_diagnostic() {
    let actual = compile_failing_case("tests/ui-printability/derive_non_printable_field_draw.rs");
    let golden = if derive_bound_note_uses_type_parameter_wording() {
        "tests/ui-printability/expected-current.stderr"
    } else {
        "tests/ui-printability/expected-msrv.stderr"
    };
    check_against_golden(&actual, golden);
}

#[test]
fn ui() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/ui/*.rs");
}
