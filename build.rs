use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const PYTHON_VERSION: &str = "3.13";

// In order:
// * Prefer `hegel` on PATH
// * If not found, install hegel with uv
//    * Prefer `uv` on PATH
//    * If not found, install uv from installer
//
// All artifacts are installed to `OUT_DIR / hegel`.
//
// HEGEL_BINARY_PATH is exported for use by the code.

fn main() {
    let hegel_path = ensure_hegel();
    eprintln!("using hegel: {}", hegel_path.display());
    // export HEGEL_BINARY_PATH for use by our code
    println!("cargo:rustc-env=HEGEL_BINARY_PATH={}", hegel_path.display());
}

fn ensure_hegel() -> PathBuf {
    if let Some(path) = find_on_path("hegel") {
        eprintln!("found hegel on path: {}", path.display());
        return path;
    }

    let install_path = PathBuf::from(env::var("OUT_DIR").unwrap()).join("hegel");
    let venv_path = install_path.join(".venv");
    let hegel_path = venv_path.join("bin").join("hegel");

    if hegel_path.exists() {
        eprintln!("found hegel: {}", hegel_path.display());
        return hegel_path;
    }

    fs::create_dir_all(&install_path)
        .unwrap_or_else(|_| panic!("failed to create {}", install_path.display()));

    let uv_path = ensure_uv(&install_path);
    eprintln!("using uv: {}", uv_path.display());

    eprintln!("creating venv at {}", uv_path.display());
    let status = Command::new(&uv_path)
        .args(["venv", "--python", PYTHON_VERSION])
        .arg(&venv_path)
        .status()
        .expect("failed to create venv");
    assert!(status.success(), "failed to create venv");

    eprintln!("installing hegel");
    let status = Command::new(&uv_path)
        .args([
            "pip",
            "install",
            "git+ssh://git@github.com/antithesishq/hegel.git@main",
        ])
        .arg("--python")
        .arg(venv_path.join("bin").join("python"))
        .status()
        .expect("failed to install hegel");
    assert!(status.success(), "failed to install hegel");
    assert!(
        hegel_path.exists(),
        "hegel not found after installation: {}",
        hegel_path.display()
    );

    hegel_path
}

fn ensure_uv(install_path: &Path) -> PathBuf {
    if let Some(path) = find_on_path("uv") {
        eprintln!("found uv on PATH: {}", path.display());
        return path;
    }

    let uv_path = install_path.join("uv");
    if uv_path.exists() {
        eprintln!("found uv: {}", uv_path.display());
        return uv_path;
    }

    eprintln!("installing uv");
    let status = Command::new("sh")
        .arg("-c")
        .arg(format!(
            "curl -LsSf https://astral.sh/uv/install.sh | UV_INSTALL_DIR={} UV_NO_MODIFY_PATH=1 sh",
            install_path.display()
        ))
        .status()
        .expect("uv install script failed");
    assert!(status.success(), "uv install script failed");
    assert!(
        uv_path.exists(),
        "uv not found at {} after installation",
        uv_path.display()
    );

    uv_path
}

fn find_on_path(name: &str) -> Option<PathBuf> {
    env::var_os("PATH").and_then(|paths| {
        env::split_paths(&paths)
            .filter_map(|dir| {
                let full_path = dir.join(name);
                if full_path.is_file() {
                    Some(full_path)
                } else {
                    None
                }
            })
            .next()
    })
}
