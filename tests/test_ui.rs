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

/// rustc also changed how it renders the "the trait `X` is not implemented
/// for `Y`" help when `Y` is a local type: the MSRV toolchain prints it as an
/// inline `= help:` note where newer toolchains print a `help:` block with a
/// source pointer at `Y`'s definition (and spell out trait paths more
/// fully). Probed like [`e0283_note_uses_must_implement_wording`].
fn trait_help_has_source_pointer() -> bool {
    let dir = tempfile::tempdir().unwrap();
    let probe = dir.path().join("probe.rs");
    std::fs::write(
        &probe,
        "#[diagnostic::on_unimplemented(message = \"probe\", label = \"probe\")]\n\
         trait Marker {}\n\
         struct Plain;\n\
         fn need<T: Marker>() {}\n\
         fn main() { need::<Plain>(); }\n",
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
        "the trait-help probe unexpectedly compiled"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    if stderr.contains("= help: the trait `Marker` is not implemented") {
        return false;
    }
    if stderr.contains("help: the trait `Marker` is not implemented") {
        return true;
    }
    panic!("unrecognized trait-help wording; add a golden for it:\n{stderr}");
}

fn rustc_binary() -> std::ffi::OsString {
    std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into())
}

/// The directories holding the compiled crates of the build this test binary
/// belongs to: the `libhegel-<hash>.rlib` the case must compile against and
/// the dependency rlibs rustc needs to load alongside it. The classic cargo
/// layout puts every compiled crate in `target/<profile>/deps` and runs this
/// binary from there too; nightly cargo's per-unit layout runs it from a
/// `target/<profile>/build/<pkg>/<hash>/out` directory and scatters the
/// rlibs across such directories. Only the running binary's own layout is
/// searched: crates laid out the other way were built by another toolchain,
/// and its rlibs may not be loadable by the rustc compiling the case.
fn crate_search_dirs() -> Vec<PathBuf> {
    fn holds_compiled_crates(dir: &Path) -> bool {
        std::fs::read_dir(dir).is_ok_and(|entries| {
            entries.filter_map(|entry| entry.ok()).any(|entry| {
                let name = entry.file_name();
                let name = name.to_string_lossy();
                name.ends_with(".rlib")
                    || name.ends_with(".rmeta")
                    || name.ends_with(".dylib")
                    || name.ends_with(".so")
                    || name.ends_with(".dll")
            })
        })
    }
    let exe = std::env::current_exe().unwrap();
    let exe_dir = exe.parent().unwrap();
    if exe_dir.file_name().is_some_and(|n| n == "deps") {
        return vec![exe_dir.to_path_buf()];
    }
    let build_dir = exe_dir.ancestors().nth(3).filter(|dir| {
        exe_dir.file_name().is_some_and(|n| n == "out")
            && dir.file_name().is_some_and(|n| n == "build")
    });
    let build_dir = build_dir
        .unwrap_or_else(|| panic!("unrecognized cargo target layout at {}", exe.display()));
    let mut dirs = Vec::new();
    for pkg in std::fs::read_dir(build_dir)
        .unwrap()
        .filter_map(|entry| entry.ok())
    {
        for unit in std::fs::read_dir(pkg.path())
            .into_iter()
            .flatten()
            .filter_map(|entry| entry.ok())
        {
            let out = unit.path().join("out");
            if holds_compiled_crates(&out) {
                dirs.push(out);
            }
        }
    }
    dirs
}

/// The most recently built `libhegel` rlib in `dirs`: stale rlibs from
/// earlier builds (other feature sets, older sources) can sit alongside it,
/// and the one cargo built or refreshed for this test run is the newest.
fn newest_hegel_rlib(dirs: &[PathBuf]) -> PathBuf {
    dirs.iter()
        .filter_map(|dir| std::fs::read_dir(dir).ok())
        .flatten()
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|path| {
            path.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("libhegel-") && n.ends_with(".rlib"))
        })
        .max_by_key(|path| std::fs::metadata(path).unwrap().modified().unwrap())
        .unwrap_or_else(|| panic!("no libhegel rlib found under {dirs:?}"))
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
///   the `--explain` hint carry no information about hegel and are dropped,
///   as are the "consider manually implementing" help for derive-introduced
///   bounds and its "to learn more" link, whose wording is still evolving
///   across toolchains;
/// - a type too long for a `required for` note is elided as `, ...>` by
///   stable but `, _>` by nightly; the nightly form is rewritten to
///   stable's.
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
            || trimmed.starts_with("= help: consider manually implementing")
            || trimmed.starts_with("= note: to learn more, visit")
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
        if trimmed.starts_with("= note: required for ") {
            out.push(format!(" {}", trimmed.replace(", _>", ", ...>")));
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
    let search_dirs = crate_search_dirs();
    let rlib = newest_hegel_rlib(&search_dirs);
    let out_dir = tempfile::tempdir().unwrap();
    let mut command = Command::new(rustc_binary());
    command
        .args(["--edition", "2021", "--emit=metadata", "--color=never"])
        .arg("--extern")
        .arg({
            let mut arg = std::ffi::OsString::from("hegel=");
            arg.push(&rlib);
            arg
        });
    let rmeta = rlib.with_extension("rmeta");
    if rmeta.exists() {
        command.arg("--extern").arg({
            let mut arg = std::ffi::OsString::from("hegel=");
            arg.push(&rmeta);
            arg
        });
    }
    for dir in &search_dirs {
        command.arg("-L").arg({
            let mut arg = std::ffi::OsString::from("dependency=");
            arg.push(dir);
            arg
        });
    }
    let output = command
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
/// or rewrite the golden under `TRYBUILD=overwrite`. Goldens are compared
/// with `\n` endings — a Windows checkout under `core.autocrlf` hands them
/// back with `\r\n`.
fn check_against_golden(actual: &str, golden: &str) {
    if std::env::var_os("TRYBUILD").is_some_and(|v| v == "overwrite") {
        std::fs::write(golden, actual).unwrap();
        return;
    }
    let expected = std::fs::read_to_string(golden)
        .unwrap_or_else(|_| panic!("missing golden {golden}; regenerate with TRYBUILD=overwrite"))
        .replace("\r\n", "\n");
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
        "tests/ui-printability/derive_non_printable_field_draw-current.stderr"
    } else {
        "tests/ui-printability/derive_non_printable_field_draw-msrv.stderr"
    };
    check_against_golden(&actual, golden);
}

/// The error a user sees when a `#[derive(PrettyPrintable)]` field's type is
/// not printable. Pinned because this diagnostic is the discovery path for
/// `#[pretty(debug)]`: it must point at the offending field and give the
/// derive-specific fixes, not draw-site advice. Checked by hand because it
/// enumerates `PrettyPrintable` implementors, which vary with the feature
/// set and toolchain.
#[test]
fn derive_non_printable_field_diagnostic() {
    let actual = compile_failing_case("tests/ui-printability/derive_non_printable_field.rs");
    let golden = if trait_help_has_source_pointer() {
        "tests/ui-printability/derive_non_printable_field-current.stderr"
    } else {
        "tests/ui-printability/derive_non_printable_field-msrv.stderr"
    };
    check_against_golden(&actual, golden);
}

/// A `one_of!` over non-printable components passed to `tc.draw`. Checked by
/// hand for the same reason as the E0283 case — the diagnostic enumerates
/// `PrettyPrintable` implementors, which vary with the feature set and
/// toolchain — but with the long-type elision normalized the rest of the
/// wording is toolchain-stable, so a single golden suffices.
#[test]
fn one_of_non_printable_draw_diagnostic() {
    let actual = compile_failing_case("tests/ui-printability/one_of_non_printable_draw.rs");
    check_against_golden(
        &actual,
        "tests/ui-printability/one_of_non_printable_draw.stderr",
    );
}

#[test]
fn ui() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/ui/*.rs");
}
