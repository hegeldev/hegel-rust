//! Runtime loading of the `libhegel_c` shared library.
//!
//! The library is searched for in this order, and the first hit wins:
//!
//! 1. The directory named by the `HEGEL_C_LIB_DIR` environment variable.
//!    Setting it makes it authoritative: nothing else is tried.
//! 2. The directory containing the running executable, so a deployed binary
//!    works with the library shipped alongside it.
//! 3. The directory `build.rs` built the engine into, baked in at compile
//!    time, which is what makes `cargo test` and `cargo run` just work.
//! 4. The platform's own library search, by loading the bare file name, which
//!    honours `LD_LIBRARY_PATH`, rpaths, ldconfig directories, `DYLD_*`, and
//!    `PATH` on Windows. This comes last so a system-installed engine can
//!    never shadow the version-matched copies above.
//!
//! Loading happens once, on the first engine call. The loaded library's
//! `hegel_version` must match the engine version this crate was built
//! against, and every `hegel_*` symbol is resolved eagerly, so an
//! incompatible library fails immediately rather than mid-run. The library
//! is never unloaded.

use std::ffi::{CStr, c_char, c_void};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use super::types::*;
use crate::control::hegel_internal_assert;

#[cfg(target_os = "windows")]
pub(super) const LIB_FILE_NAME: &str = "hegel_c.dll";
#[cfg(target_os = "macos")]
pub(super) const LIB_FILE_NAME: &str = "libhegel_c.dylib";
#[cfg(all(unix, not(target_os = "macos")))]
pub(super) const LIB_FILE_NAME: &str = "libhegel_c.so";

#[derive(Debug)]
pub(super) struct Library {
    handle: *mut c_void,
    path: PathBuf,
}

pub(super) fn candidate_paths(
    env_dir: Option<PathBuf>,
    exe_dir: Option<PathBuf>,
    baked_dir: &str,
) -> Vec<PathBuf> {
    if let Some(dir) = env_dir {
        return vec![dir.join(LIB_FILE_NAME)];
    }
    let mut paths = Vec::new();
    if let Some(dir) = exe_dir {
        paths.push(dir.join(LIB_FILE_NAME));
    }
    if !baked_dir.is_empty() {
        paths.push(Path::new(baked_dir).join(LIB_FILE_NAME));
    }
    paths.push(PathBuf::from(LIB_FILE_NAME));
    paths
}

fn is_bare_name(path: &Path) -> bool {
    path.parent().is_some_and(|p| p.as_os_str().is_empty())
}

pub(super) fn load_from(paths: &[PathBuf]) -> Result<Library, String> {
    let mut tried = String::new();
    for path in paths {
        match platform::open(path) {
            Ok(handle) => {
                return Ok(Library {
                    handle,
                    path: path.clone(),
                });
            }
            Err(err) if is_bare_name(path) => tried.push_str(&format!(
                "  {} (system library search): {err}\n",
                path.display()
            )),
            Err(err) => tried.push_str(&format!("  {}: {err}\n", path.display())),
        }
    }
    Err(format!(
        "could not load the libhegel engine library ({LIB_FILE_NAME}). Tried:\n{tried}Set \
         HEGEL_C_LIB_DIR to the directory containing the library, place the library next to \
         the running executable, or enable hegeltest's `static-engine` feature to link the \
         engine into the binary instead."
    ))
}

pub(super) fn load_library() -> Library {
    let env_dir = std::env::var_os("HEGEL_C_LIB_DIR").map(PathBuf::from);
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(Path::to_path_buf));
    let paths = candidate_paths(env_dir, exe_dir, env!("HEGEL_C_BAKED_LIB_DIR"));
    let lib = load_from(&paths).unwrap_or_else(|err| panic!("{err}"));
    let expected = env!("HEGEL_C_EXPECTED_VERSION");
    check_engine_version(&lib, expected).unwrap_or_else(|err| panic!("{err}"));
    lib
}

pub(super) fn engine_version(lib: &Library) -> String {
    let sym = require_symbol(lib, "hegel_version\0");
    let hegel_version = unsafe {
        std::mem::transmute::<
            *mut c_void,
            unsafe extern "C" fn(*mut HegelContext, *mut *const c_char) -> hegel_result_t,
        >(sym)
    };
    let mut version: *const c_char = std::ptr::null();
    let result = unsafe { hegel_version(std::ptr::null_mut(), &mut version) };
    hegel_internal_assert!(result == hegel_result_t::HEGEL_OK);
    hegel_internal_assert!(!version.is_null());
    unsafe { CStr::from_ptr(version) }
        .to_string_lossy()
        .into_owned()
}

pub(super) fn check_engine_version(lib: &Library, expected: &str) -> Result<(), String> {
    let found = engine_version(lib);
    if found == expected {
        return Ok(());
    }
    Err(format!(
        "{} is libhegel {found}, but this build of hegeltest requires libhegel {expected}. \
         Replace the library with the matching version, set HEGEL_C_LIB_DIR to a directory \
         containing one, or enable hegeltest's `static-engine` feature to link the engine \
         into the binary instead.",
        lib.path.display(),
    ))
}

pub(super) fn require_symbol(lib: &Library, name: &'static str) -> *mut c_void {
    let sym = platform::symbol(lib.handle, name);
    hegel_internal_assert!(name.ends_with('\0'));
    if sym.is_null() {
        panic!(
            "{} does not export {}, so it is not a compatible libhegel build",
            lib.path.display(),
            name.trim_end_matches('\0'),
        );
    }
    sym
}

macro_rules! define_engine_api {
    ($(fn $name:ident($($arg:ident: $ty:ty),* $(,)?) -> $ret:ty;)*) => {
        struct Api {
            $($name: unsafe extern "C" fn($($ty),*) -> $ret,)*
        }

        fn api() -> &'static Api {
            static API: OnceLock<Api> = OnceLock::new();
            API.get_or_init(|| {
                let lib = load_library();
                Api {
                    $($name: unsafe {
                        std::mem::transmute::<*mut c_void, unsafe extern "C" fn($($ty),*) -> $ret>(
                            require_symbol(&lib, concat!(stringify!($name), "\0")),
                        )
                    },)*
                }
            })
        }

        $(
            pub(crate) unsafe fn $name($($arg: $ty),*) -> $ret {
                unsafe { (api().$name)($($arg),*) }
            }
        )*
    };
}

super::for_each_hegel_fn!(define_engine_api);

#[cfg(unix)]
mod platform {
    use std::ffi::{CStr, CString, c_char, c_int, c_void};
    use std::path::Path;

    use crate::control::hegel_internal_assert;

    unsafe extern "C" {
        fn dlopen(filename: *const c_char, flags: c_int) -> *mut c_void;
        fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
        fn dlerror() -> *mut c_char;
    }

    #[cfg(target_os = "macos")]
    const RTLD_NOW_LOCAL: c_int = 0x2 | 0x4;
    #[cfg(not(target_os = "macos"))]
    const RTLD_NOW_LOCAL: c_int = 0x2;

    pub(super) fn open(path: &Path) -> Result<*mut c_void, String> {
        use std::os::unix::ffi::OsStrExt;
        let path = CString::new(path.as_os_str().as_bytes())
            .map_err(|_| "path contains an interior NUL byte".to_owned())?;
        let handle = unsafe { dlopen(path.as_ptr(), RTLD_NOW_LOCAL) };
        if handle.is_null() {
            return Err(last_error());
        }
        Ok(handle)
    }

    pub(super) fn symbol(handle: *mut c_void, name: &'static str) -> *mut c_void {
        unsafe { dlsym(handle, name.as_ptr().cast::<c_char>()) }
    }

    fn last_error() -> String {
        let err = unsafe { dlerror() };
        hegel_internal_assert!(!err.is_null());
        unsafe { CStr::from_ptr(err) }
            .to_string_lossy()
            .into_owned()
    }
}

#[cfg(windows)]
mod platform {
    #![allow(non_snake_case)]

    use std::ffi::c_void;
    use std::path::Path;

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn LoadLibraryW(file_name: *const u16) -> *mut c_void;
        fn GetProcAddress(module: *mut c_void, name: *const u8) -> *mut c_void;
        fn GetLastError() -> u32;
    }

    pub(super) fn open(path: &Path) -> Result<*mut c_void, String> {
        use std::os::windows::ffi::OsStrExt;
        let wide: Vec<u16> = path.as_os_str().encode_wide().chain([0]).collect();
        let handle = unsafe { LoadLibraryW(wide.as_ptr()) };
        if handle.is_null() {
            return Err(format!("LoadLibraryW failed with error code {}", unsafe {
                GetLastError()
            }));
        }
        Ok(handle)
    }

    pub(super) fn symbol(handle: *mut c_void, name: &'static str) -> *mut c_void {
        unsafe { GetProcAddress(handle, name.as_ptr()) }
    }
}
