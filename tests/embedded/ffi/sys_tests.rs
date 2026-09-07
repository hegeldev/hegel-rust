//! Embedded tests for `crate::ffi::sys`, the two-mode `hegel_*` boundary.
//!
//! In the default (shared-library) mode these exercise the loader: candidate
//! ordering and every load and symbol-resolution failure path, against real
//! `dlopen`/`LoadLibraryW` calls. With `static-engine` enabled they instead
//! compile-assert that the [`for_each_hegel_fn`] list and the mirrored types
//! in `types.rs` match the real `hegel_c` definitions, so any engine-side
//! signature or layout change fails the all-features build at compile time.

#[cfg(not(feature = "static-engine"))]
mod loading {
    use crate::ffi::sys::loader;
    use std::path::PathBuf;

    #[test]
    fn an_env_dir_is_authoritative() {
        let paths = loader::candidate_paths(
            Some(PathBuf::from("/env")),
            Some(PathBuf::from("/exe")),
            "/baked",
        );
        assert_eq!(
            paths,
            vec![PathBuf::from("/env").join(loader::LIB_FILE_NAME)]
        );
    }

    #[test]
    fn the_exe_dir_then_the_baked_dir_come_before_the_system_search() {
        let paths = loader::candidate_paths(None, Some(PathBuf::from("/exe")), "/baked");
        assert_eq!(
            paths,
            vec![
                PathBuf::from("/exe").join(loader::LIB_FILE_NAME),
                PathBuf::from("/baked").join(loader::LIB_FILE_NAME),
                PathBuf::from(loader::LIB_FILE_NAME),
            ]
        );
    }

    #[test]
    fn without_explicit_candidates_only_the_system_search_remains() {
        assert_eq!(
            loader::candidate_paths(None, None, ""),
            vec![PathBuf::from(loader::LIB_FILE_NAME)]
        );
    }

    #[test]
    fn a_failed_load_reports_every_candidate_and_the_remedies() {
        let empty = tempfile::tempdir().unwrap();
        let garbage = tempfile::tempdir().unwrap();
        std::fs::write(garbage.path().join(loader::LIB_FILE_NAME), b"not a library").unwrap();
        let paths = [
            empty.path().join(loader::LIB_FILE_NAME),
            garbage.path().join(loader::LIB_FILE_NAME),
            PathBuf::from("hegel_no_such_engine_library"),
        ];
        let err = loader::load_from(&paths).unwrap_err();
        for path in &paths {
            assert!(err.contains(&path.display().to_string()), "{err}");
        }
        assert!(
            err.contains("hegel_no_such_engine_library (system library search)"),
            "{err}"
        );
        assert!(err.contains("HEGEL_C_LIB_DIR"), "{err}");
        assert!(err.contains("static-engine"), "{err}");
    }

    #[cfg(unix)]
    #[test]
    fn a_candidate_path_with_an_interior_nul_byte_is_reported() {
        use std::os::unix::ffi::OsStringExt;
        let path = PathBuf::from(std::ffi::OsString::from_vec(b"nul\0dir".to_vec()));
        let err = loader::load_from(&[path]).unwrap_err();
        assert!(err.contains("NUL"), "{err}");
    }

    #[test]
    fn the_loaded_library_reports_the_expected_version() {
        let lib = loader::load_library();
        assert_eq!(
            loader::engine_version(&lib),
            env!("HEGEL_C_EXPECTED_VERSION")
        );
    }

    #[test]
    fn a_version_mismatch_names_the_library_and_both_versions() {
        let lib = loader::load_library();
        let err = loader::check_engine_version(&lib, "999.999.999").unwrap_err();
        assert!(err.contains("999.999.999"), "{err}");
        assert!(err.contains(env!("HEGEL_C_EXPECTED_VERSION")), "{err}");
        assert!(err.contains(loader::LIB_FILE_NAME), "{err}");
        assert!(err.contains("HEGEL_C_LIB_DIR"), "{err}");
        assert!(err.contains("static-engine"), "{err}");
    }

    #[test]
    fn an_incompatible_library_is_reported_by_symbol_name() {
        let lib = loader::load_library();
        let panic =
            std::panic::catch_unwind(|| loader::require_symbol(&lib, "hegel_no_such_symbol\0"))
                .unwrap_err();
        let msg = panic.downcast_ref::<String>().unwrap();
        assert!(msg.contains("hegel_no_such_symbol"), "{msg}");
        assert!(msg.contains(loader::LIB_FILE_NAME), "{msg}");
    }
}

#[cfg(feature = "static-engine")]
mod drift {
    use crate::ffi::sys::types as mirror;
    use hegel_c::*;
    use std::ffi::{c_char, c_void};
    use std::mem::offset_of;

    macro_rules! assert_fn_signatures_match {
        ($(fn $name:ident($($arg:ident: $ty:ty),* $(,)?) -> $ret:ty;)*) => {
            $(const _: unsafe extern "C" fn($($ty),*) -> $ret = hegel_c::$name;)*
        };
    }
    crate::ffi::sys::for_each_hegel_fn!(assert_fn_signatures_match);

    macro_rules! assert_enum_mirrors {
        ($($name:ident { $($variant:ident),* $(,)? })*) => {
            const _: () = {
                $(
                    assert!(size_of::<mirror::$name>() == size_of::<$name>());
                    assert!(align_of::<mirror::$name>() == align_of::<$name>());
                    $(assert!(mirror::$name::$variant as i64 == $name::$variant as i64);)*
                )*
            };
        };
    }

    assert_enum_mirrors! {
        hegel_result_t {
            HEGEL_OK,
            HEGEL_E_STOP_TEST,
            HEGEL_E_ASSUME,
            HEGEL_E_BACKEND,
            HEGEL_E_INVALID_HANDLE,
            HEGEL_E_INVALID_ARG,
            HEGEL_E_ALREADY_COMPLETE,
            HEGEL_E_NOT_COMPLETE,
            HEGEL_E_INTERNAL,
            HEGEL_E_CONCURRENT_USE,
            HEGEL_E_RETRY,
        }
        hegel_status_t {
            HEGEL_STATUS_VALID,
            HEGEL_STATUS_INVALID,
            HEGEL_STATUS_OVERRUN,
            HEGEL_STATUS_INTERESTING,
        }
        hegel_backend_t {
            HEGEL_BACKEND_AUTO,
            HEGEL_BACKEND_DEFAULT,
            HEGEL_BACKEND_URANDOM,
        }
        hegel_run_status_t {
            HEGEL_RUN_STATUS_PASSED,
            HEGEL_RUN_STATUS_FAILED,
            HEGEL_RUN_STATUS_ERROR,
            HEGEL_RUN_STATUS_FAILED_NONDETERMINISTIC,
        }
        hegel_verbosity_t {
            HEGEL_VERBOSITY_QUIET,
            HEGEL_VERBOSITY_NORMAL,
            HEGEL_VERBOSITY_VERBOSE,
            HEGEL_VERBOSITY_DEBUG,
        }
        hegel_phase_t {
            HEGEL_PHASE_EXPLICIT,
            HEGEL_PHASE_REUSE,
            HEGEL_PHASE_GENERATE,
            HEGEL_PHASE_TARGET,
            HEGEL_PHASE_SHRINK,
            HEGEL_PHASE_ALL,
        }
        hegel_health_check_t {
            HEGEL_HC_FILTER_TOO_MUCH,
            HEGEL_HC_TOO_SLOW,
            HEGEL_HC_TEST_CASES_TOO_LARGE,
            HEGEL_HC_LARGE_INITIAL_TEST_CASE,
        }
        hegel_label_t {
            HEGEL_LABEL_LIST,
            HEGEL_LABEL_LIST_ELEMENT,
            HEGEL_LABEL_SET,
            HEGEL_LABEL_SET_ELEMENT,
            HEGEL_LABEL_MAP,
            HEGEL_LABEL_MAP_ENTRY,
            HEGEL_LABEL_TUPLE,
            HEGEL_LABEL_ONE_OF,
            HEGEL_LABEL_OPTIONAL,
            HEGEL_LABEL_FIXED_DICT,
            HEGEL_LABEL_FLAT_MAP,
            HEGEL_LABEL_FILTER,
            HEGEL_LABEL_MAPPED,
            HEGEL_LABEL_SAMPLED_FROM,
            HEGEL_LABEL_ENUM_VARIANT,
            HEGEL_LABEL_FEATURE_FLAG,
            HEGEL_LABEL_REGEX,
            HEGEL_LABEL_EMAIL,
            HEGEL_LABEL_URL,
            HEGEL_LABEL_DOMAIN,
            HEGEL_LABEL_DATE,
            HEGEL_LABEL_TIME,
            HEGEL_LABEL_DATETIME,
            HEGEL_LABEL_UUID,
            HEGEL_LABEL_IP_ADDRESS,
            HEGEL_LABEL_INTEGER,
            HEGEL_LABEL_FLOAT,
            HEGEL_LABEL_BOOLEAN,
            HEGEL_LABEL_BYTES,
            HEGEL_LABEL_STRING,
            HEGEL_LABEL_STATEFUL_RULE,
            HEGEL_LABEL_FRESH_ID,
            HEGEL_LABEL_SET_CHOICE,
            HEGEL_LABEL_CONCURRENCY,
            HEGEL_LABEL_RECURSIVE,
        }
    }

    macro_rules! assert_struct_mirrors {
        ($($name:ident { $($field:ident),* $(,)? })*) => {
            const _: () = {
                $(
                    assert!(size_of::<mirror::$name>() == size_of::<$name>());
                    assert!(align_of::<mirror::$name>() == align_of::<$name>());
                    $(assert!(offset_of!(mirror::$name, $field) == offset_of!($name, $field));)*
                )*
            };
        };
    }

    assert_struct_mirrors! {
        hegel_date_t { year, month, day }
        hegel_time_t { hour, minute, second, nanosecond }
        hegel_datetime_t { date, time }
        hegel_generate_bytes_result_t { data, len }
        hegel_generate_string_result_t { data, len }
        hegel_printer_value_result_t { data, len }
    }

    const _: () = {
        assert!(mirror::HEGEL_STATE_MACHINE_DONE == HEGEL_STATE_MACHINE_DONE);
        assert!(
            size_of::<mirror::hegel_output_callback_t>() == size_of::<hegel_output_callback_t>()
        );
    };

    const _: unsafe extern "C" fn(*mut HegelContext, *mut *const c_char) -> hegel_result_t =
        hegel_version;
}
