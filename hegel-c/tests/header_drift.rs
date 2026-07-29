//! Checks that the committed `include/hegel.h` matches what cbindgen
//! generates from the current source. `HEGEL_C_HEADER_WRITE=1` (via `just
//! c-header`) rewrites the header instead of comparing.

use std::env;
use std::fs;
use std::path::PathBuf;

#[test]
fn header_matches_source() {
    let crate_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let header_path = crate_dir.join("include").join("hegel.h");

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
        fs::write(&header_path, &new_text).expect("write header");
        return;
    }

    let existing = fs::read_to_string(&header_path)
        .unwrap_or_default()
        .replace("\r\n", "\n");
    assert!(
        existing == new_text.replace("\r\n", "\n"),
        "include/hegel.h is out of date. Run `just c-header` to refresh it."
    );
}
