// Regenerate include/hegel.h from src/lib.rs on every build, and fail
// the build if the checked-in copy is stale. Catches drift in CI without
// requiring a separate header-regen step.

use std::env;
use std::fs;
use std::path::PathBuf;

fn main() {
    let crate_dir = env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR set by cargo");
    let crate_dir = PathBuf::from(crate_dir);
    let header_path = crate_dir.join("include").join("hegel.h");

    // Tell cargo to rerun if the source or config changes.
    println!("cargo:rerun-if-changed=src/lib.rs");
    println!("cargo:rerun-if-changed=cbindgen.toml");

    let config = cbindgen::Config::from_file(crate_dir.join("cbindgen.toml"))
        .expect("loading cbindgen.toml");

    let generated = cbindgen::Builder::new()
        .with_crate(&crate_dir)
        .with_config(config)
        .generate()
        .expect("generating header");

    let mut new_text = Vec::new();
    generated.write(&mut new_text);
    let new_text = String::from_utf8(new_text).expect("cbindgen emits UTF-8");

    if env::var_os("HEGEL_C_HEADER_WRITE").is_some() {
        fs::create_dir_all(header_path.parent().unwrap()).expect("create include/");
        fs::write(&header_path, &new_text).expect("write header");
        return;
    }

    let existing = fs::read_to_string(&header_path).unwrap_or_default();
    if existing != new_text {
        panic!(
            "include/hegel.h is out of date. Run `HEGEL_C_HEADER_WRITE=1 cargo build -p hegeltest-c` to refresh it."
        );
    }
}
