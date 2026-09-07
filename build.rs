//! Provides the `libhegel_c` shared library the default (non-`static-engine`)
//! build loads at runtime.
//!
//! In order of preference: skip entirely when `static-engine` links the
//! engine as a Rust dependency (or on docs.rs, which cannot build it); trust
//! an explicit `HEGEL_C_LIB_DIR`; otherwise build the engine cdylib with a
//! nested cargo invocation — from the sibling `hegel-c/` directory in a git
//! checkout, or from the crates.io `hegeltest-c` release this version pins
//! when building the published crate. The nested build happens through a
//! generated shim crate because a cdylib must be a top-level target: cargo
//! never builds the cdylib of a dependency. The resulting directory is baked
//! into the frontend as `HEGEL_C_BAKED_LIB_DIR`, the loader's fallback
//! search path. The pinned engine version is always baked in as
//! `HEGEL_C_EXPECTED_VERSION`, which the loader checks against the loaded
//! library's `hegel_version`.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=Cargo.toml");
    println!("cargo:rerun-if-env-changed=HEGEL_C_LIB_DIR");
    println!("cargo:rerun-if-env-changed=DOCS_RS");

    let manifest_dir = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap());
    println!(
        "cargo:rustc-env=HEGEL_C_EXPECTED_VERSION={}",
        pinned_engine_version(&manifest_dir).trim_start_matches('=')
    );

    if env::var_os("CARGO_FEATURE_STATIC_ENGINE").is_some() || env::var_os("DOCS_RS").is_some() {
        println!("cargo:rustc-env=HEGEL_C_BAKED_LIB_DIR=");
        return;
    }

    if env::var("CARGO_CFG_TARGET_OS").unwrap() == "linux"
        && env::var("CARGO_CFG_TARGET_ENV").unwrap() == "gnu"
    {
        println!("cargo:rustc-link-lib=dylib=dl");
    }

    let lib_dir = match env::var("HEGEL_C_LIB_DIR") {
        Ok(dir) => dir,
        Err(_) => build_engine().display().to_string(),
    };
    println!("cargo:rustc-env=HEGEL_C_BAKED_LIB_DIR={lib_dir}");
}

fn build_engine() -> PathBuf {
    let out_dir = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let manifest_dir = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap());

    let sibling = manifest_dir.join("hegel-c");
    let dependency = if sibling.join("Cargo.toml").exists() {
        println!("cargo:rerun-if-changed={}", sibling.join("src").display());
        println!(
            "cargo:rerun-if-changed={}",
            sibling.join("Cargo.toml").display()
        );
        format!("path = {:?}", sibling.to_str().unwrap())
    } else {
        format!("version = {:?}", pinned_engine_version(&manifest_dir))
    };

    let shim_dir = out_dir.join("engine-shim");
    fs::create_dir_all(shim_dir.join("src")).unwrap();
    fs::write(
        shim_dir.join("Cargo.toml"),
        format!(
            r#"[package]
name = "hegel-engine-shim"
version = "0.0.0"
edition = "2024"

[lib]
name = "hegel_engine_shim"
crate-type = ["cdylib"]

[dependencies]
hegeltest-c = {{ {dependency} }}

[profile.dev]
opt-level = 1
debug = "line-tables-only"

[workspace]
"#
        ),
    )
    .unwrap();
    fs::write(shim_dir.join("src/lib.rs"), "pub use hegel_c::*;\n").unwrap();

    let target = env::var("TARGET").unwrap();
    let release = env::var("PROFILE").unwrap() == "release";
    let target_dir = out_dir.join("engine-target");

    let mut cargo = Command::new(env::var_os("CARGO").unwrap());
    cargo
        .arg("build")
        .arg("--manifest-path")
        .arg(shim_dir.join("Cargo.toml"))
        .arg("--target-dir")
        .arg(&target_dir)
        .arg("--target")
        .arg(&target);
    if release {
        cargo.arg("--release");
    }
    for (name, _) in env::vars_os() {
        let name = name.to_string_lossy().into_owned();
        if name.starts_with("CARGO_PROFILE_") {
            cargo.env_remove(name);
        }
    }
    for name in [
        "RUSTFLAGS",
        "CARGO_ENCODED_RUSTFLAGS",
        "CARGO_BUILD_RUSTFLAGS",
        "RUSTC_WORKSPACE_WRAPPER",
        "MAKEFLAGS",
        "CARGO_MAKEFLAGS",
    ] {
        cargo.env_remove(name);
    }

    let output = cargo.output().unwrap();
    if !output.status.success() {
        panic!(
            "building the libhegel engine failed ({}):\n{}\n{}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
    }

    let profile_dir = if release { "release" } else { "debug" };
    let built = target_dir
        .join(&target)
        .join(profile_dir)
        .join(shim_artifact_name());
    let lib_dir = out_dir.join("lib");
    fs::create_dir_all(&lib_dir).unwrap();
    let installed = lib_dir.join(engine_lib_name());
    fs::copy(&built, &installed).unwrap_or_else(|err| {
        panic!(
            "copying {} to {}: {err}",
            built.display(),
            installed.display()
        )
    });
    lib_dir
}

fn pinned_engine_version(manifest_dir: &Path) -> String {
    let manifest = fs::read_to_string(manifest_dir.join("Cargo.toml")).unwrap();
    let mut in_dep_table = false;
    for line in manifest.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_dep_table = line == "[dependencies.hegeltest-c]";
        }
        let has_version = (in_dep_table && line.starts_with("version"))
            || (line.starts_with("hegeltest-c") && line.contains("version"));
        if has_version {
            let (_, rest) = line.split_once("version").unwrap();
            return rest.split('"').nth(1).unwrap().to_owned();
        }
    }
    panic!(
        "no hegeltest-c version pin found in {}",
        manifest_dir.join("Cargo.toml").display()
    );
}

fn shim_artifact_name() -> String {
    match env::var("CARGO_CFG_TARGET_OS").unwrap().as_str() {
        "windows" => "hegel_engine_shim.dll".to_owned(),
        "macos" => "libhegel_engine_shim.dylib".to_owned(),
        _ => "libhegel_engine_shim.so".to_owned(),
    }
}

fn engine_lib_name() -> String {
    match env::var("CARGO_CFG_TARGET_OS").unwrap().as_str() {
        "windows" => "hegel_c.dll".to_owned(),
        "macos" => "libhegel_c.dylib".to_owned(),
        _ => "libhegel_c.so".to_owned(),
    }
}
