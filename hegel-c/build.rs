use std::env;
use std::fs;
use std::path::PathBuf;

fn main() {
    let crate_dir = env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR set by cargo");
    let crate_dir = PathBuf::from(crate_dir);
    let header_path = crate_dir.join("include").join("hegel.h");

    println!("cargo:rerun-if-changed=src/lib.rs");
    println!("cargo:rerun-if-changed=cbindgen.toml");

    if crate_dir.components().any(|c| c.as_os_str() == "package")
        && crate_dir.components().any(|c| c.as_os_str() == "target")
    {
        return;
    }

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

    let existing_raw = fs::read_to_string(&header_path).unwrap_or_default();
    let existing = existing_raw.replace("\r\n", "\n");
    let new_text_lf = new_text.replace("\r\n", "\n");
    if existing != new_text_lf {
        panic!(
            "include/hegel.h is out of date. Run `HEGEL_C_HEADER_WRITE=1 cargo build -p hegeltest-c` to refresh it."
        );
    }
}
