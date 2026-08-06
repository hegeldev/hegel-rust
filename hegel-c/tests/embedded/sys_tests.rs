use alloc::format;
use alloc::string::ToString;
use alloc::vec;
use core::time::Duration;

use tempfile::TempDir;

use super::*;

fn temp_path(dir: &TempDir, name: &str) -> String {
    format!("{}/{}", dir.path().to_str().unwrap(), name)
}

#[test]
fn fs_write_read_round_trips() {
    let dir = TempDir::new().unwrap();
    let path = temp_path(&dir, "file");
    fs::write(&path, b"contents").unwrap();
    assert_eq!(fs::read(&path).unwrap(), b"contents".to_vec());
}

#[test]
fn fs_write_truncates_an_existing_file() {
    let dir = TempDir::new().unwrap();
    let path = temp_path(&dir, "file");
    fs::write(&path, b"a much longer first draft").unwrap();
    fs::write(&path, b"short").unwrap();
    assert_eq!(fs::read(&path).unwrap(), b"short".to_vec());
}

#[test]
fn fs_read_of_missing_file_fails() {
    let dir = TempDir::new().unwrap();
    assert!(fs::read(&temp_path(&dir, "missing")).is_err());
}

#[test]
fn fs_write_into_missing_directory_fails() {
    let dir = TempDir::new().unwrap();
    assert!(fs::write(&temp_path(&dir, "no-such-dir/file"), b"x").is_err());
}

#[test]
fn fs_create_dir_all_builds_nested_directories() {
    let dir = TempDir::new().unwrap();
    let nested = temp_path(&dir, "a/b/c");
    fs::create_dir_all(&nested).unwrap();
    assert!(fs::exists(&nested));
    fs::write(&format!("{nested}/file"), b"x").unwrap();
}

#[test]
fn fs_create_dir_all_is_idempotent() {
    let dir = TempDir::new().unwrap();
    let nested = temp_path(&dir, "a/b");
    fs::create_dir_all(&nested).unwrap();
    fs::create_dir_all(&nested).unwrap();
    assert!(fs::exists(&nested));
}

#[test]
fn fs_create_dir_all_of_an_existing_file_reports_success() {
    let dir = TempDir::new().unwrap();
    let file = temp_path(&dir, "file");
    fs::write(&file, b"x").unwrap();
    fs::create_dir_all(&file).unwrap();
    assert_eq!(fs::read(&file).unwrap(), b"x".to_vec());
}

#[test]
fn fs_create_dir_all_fails_under_a_file() {
    let dir = TempDir::new().unwrap();
    let file = temp_path(&dir, "file");
    fs::write(&file, b"x").unwrap();
    assert!(fs::create_dir_all(&format!("{file}/sub")).is_err());
}

#[test]
fn fs_read_dir_lists_entry_names() {
    let dir = TempDir::new().unwrap();
    fs::write(&temp_path(&dir, "one"), b"1").unwrap();
    fs::write(&temp_path(&dir, "two"), b"2").unwrap();
    let mut names = fs::read_dir(dir.path().to_str().unwrap()).unwrap();
    names.sort();
    assert_eq!(names, vec!["one".to_string(), "two".to_string()]);
}

#[test]
fn fs_read_dir_of_missing_directory_fails() {
    let dir = TempDir::new().unwrap();
    assert!(fs::read_dir(&temp_path(&dir, "missing")).is_err());
}

#[test]
fn fs_read_dir_of_a_file_fails() {
    let dir = TempDir::new().unwrap();
    let file = temp_path(&dir, "file");
    fs::write(&file, b"x").unwrap();
    assert!(fs::read_dir(&file).is_err());
}

#[cfg(target_os = "linux")]
#[test]
fn fs_read_dir_skips_non_unicode_names() {
    use std::os::unix::ffi::OsStrExt;
    let dir = TempDir::new().unwrap();
    fs::write(&temp_path(&dir, "plain"), b"x").unwrap();
    let weird = dir
        .path()
        .join(std::ffi::OsStr::from_bytes(&[0x66, 0xff, 0xfe]));
    std::fs::write(&weird, b"y").unwrap();
    let names = fs::read_dir(dir.path().to_str().unwrap()).unwrap();
    assert_eq!(names, vec!["plain".to_string()]);
}

#[test]
fn fs_rename_moves_and_replaces() {
    let dir = TempDir::new().unwrap();
    let from = temp_path(&dir, "from");
    let to = temp_path(&dir, "to");
    fs::write(&from, b"new").unwrap();
    fs::write(&to, b"old").unwrap();
    fs::rename(&from, &to).unwrap();
    assert!(!fs::exists(&from));
    assert_eq!(fs::read(&to).unwrap(), b"new".to_vec());
}

#[test]
fn fs_rename_of_missing_source_fails() {
    let dir = TempDir::new().unwrap();
    assert!(fs::rename(&temp_path(&dir, "missing"), &temp_path(&dir, "to")).is_err());
}

#[test]
fn fs_remove_file_deletes_and_reports_missing() {
    let dir = TempDir::new().unwrap();
    let path = temp_path(&dir, "file");
    fs::write(&path, b"x").unwrap();
    fs::remove_file(&path).unwrap();
    assert!(!fs::exists(&path));
    assert!(fs::remove_file(&path).is_err());
}

#[test]
fn fs_remove_dir_only_removes_empty_directories() {
    let dir = TempDir::new().unwrap();
    let sub = temp_path(&dir, "sub");
    fs::create_dir_all(&sub).unwrap();
    fs::write(&format!("{sub}/file"), b"x").unwrap();
    assert!(fs::remove_dir(&sub).is_err());
    fs::remove_file(&format!("{sub}/file")).unwrap();
    fs::remove_dir(&sub).unwrap();
    assert!(!fs::exists(&sub));
}

#[test]
fn instant_now_is_monotonic() {
    let a = Instant::now().unwrap();
    let b = Instant::now().unwrap();
    assert!(b >= a);
}

#[test]
fn instant_elapsed_and_duration_since_saturate() {
    let now = Instant::now().unwrap();
    let future = now + Duration::from_secs(3600);
    assert_eq!(now.duration_since(future), Duration::ZERO);
    assert!(future.duration_since(now) >= Duration::from_secs(3600));
    assert!(now.elapsed() < Duration::from_secs(3600));
}

#[test]
fn instant_add_and_sub_order_correctly() {
    let now = Instant::now().unwrap();
    assert!(now - Duration::from_secs(1) < now);
    assert!(now + Duration::from_secs(1) > now);
    assert!(now - Duration::MAX <= now);
    assert!(now + Duration::MAX >= now);
}

#[test]
fn entropy_fills_distinct_buffers() {
    let mut a = [0u8; 16];
    let mut b = [0u8; 16];
    entropy(&mut a).unwrap();
    entropy(&mut b).unwrap();
    assert_ne!(a, b, "two 16-byte entropy draws must differ");
}

#[cfg(unix)]
#[test]
fn urandom_is_available_and_fills_distinct_buffers() {
    assert!(urandom_available());
    let mut a = [0u8; 16];
    let mut b = [0u8; 16];
    urandom(&mut a).unwrap();
    urandom(&mut b).unwrap();
    assert_ne!(a, b, "two 16-byte urandom draws must differ");
}

#[cfg(windows)]
#[test]
fn urandom_is_unavailable() {
    assert!(!urandom_available());
    let mut buf = [0u8; 4];
    assert!(urandom(&mut buf).is_err());
}

#[cfg(unix)]
#[test]
fn retry_intr_retries_interrupted_calls() {
    let mut calls = 0;
    let result = imp::retry_intr(|| {
        calls += 1;
        if calls < 3 {
            Err(rustix::io::Errno::INTR)
        } else {
            Ok(7)
        }
    });
    assert_eq!(result, Ok(7));
    assert_eq!(calls, 3);
}

#[cfg(unix)]
#[test]
fn retry_intr_passes_other_errors_through() {
    let mut calls = 0;
    let result: Result<u8, rustix::io::Errno> = imp::retry_intr(|| {
        calls += 1;
        Err(rustix::io::Errno::NOENT)
    });
    assert_eq!(result, Err(rustix::io::Errno::NOENT));
    assert_eq!(calls, 1);
}

#[cfg(unix)]
#[test]
fn fill_all_resumes_after_short_and_interrupted_fills() {
    let mut buf = [0u8; 6];
    let mut calls = 0;
    imp::fill_all(&mut buf, |chunk| {
        calls += 1;
        match calls {
            1 => Err(rustix::io::Errno::INTR),
            2 => {
                chunk[..2].copy_from_slice(b"ab");
                Ok(2)
            }
            _ => {
                let len = chunk.len();
                chunk.copy_from_slice(&b"cdef"[..len]);
                Ok(len)
            }
        }
    })
    .unwrap();
    assert_eq!(&buf, b"abcdef");
    assert_eq!(calls, 3);
}

#[cfg(unix)]
#[test]
fn fill_all_fails_on_zero_progress() {
    let mut buf = [0u8; 4];
    assert_eq!(imp::fill_all(&mut buf, |_| Ok(0)), Err(Error));
}

#[cfg(unix)]
#[test]
fn write_all_resumes_after_short_and_interrupted_writes() {
    let mut written = alloc::vec::Vec::new();
    let mut calls = 0;
    imp::write_all(b"abcdef", |chunk| {
        calls += 1;
        match calls {
            1 => Err(rustix::io::Errno::INTR),
            2 => {
                written.extend_from_slice(&chunk[..2]);
                Ok(2)
            }
            _ => {
                written.extend_from_slice(chunk);
                Ok(chunk.len())
            }
        }
    })
    .unwrap();
    assert_eq!(written, b"abcdef");
    assert_eq!(calls, 3);
}

#[cfg(unix)]
#[test]
fn write_all_fails_on_zero_progress() {
    assert_eq!(imp::write_all(b"data", |_| Ok(0)), Err(Error));
}

#[cfg(unix)]
#[test]
fn read_exact_requires_enough_bytes() {
    let dir = TempDir::new().unwrap();
    let path = temp_path(&dir, "file");
    fs::write(&path, b"1234").unwrap();
    let mut exact = [0u8; 4];
    imp::read_exact(&path, &mut exact).unwrap();
    assert_eq!(&exact, b"1234");
    let mut too_big = [0u8; 8];
    assert!(imp::read_exact(&path, &mut too_big).is_err());
    assert!(imp::read_exact(&temp_path(&dir, "missing"), &mut exact).is_err());
}

#[test]
fn env_var_reads_set_variables() {
    let expected = std::env::var("CARGO_PKG_NAME").unwrap();
    assert_eq!(env_var("CARGO_PKG_NAME"), Some(expected));
}

#[test]
fn env_var_of_unset_variable_is_none() {
    assert_eq!(env_var("HEGEL_SYS_TEST_DEFINITELY_UNSET"), None);
}

#[cfg(unix)]
#[test]
fn env_var_rejects_interior_nul() {
    assert_eq!(env_var("HEGEL\0SYS"), None);
}

#[cfg(windows)]
#[test]
fn env_var_distinguishes_empty_from_unset() {
    // SAFETY: the Win32 environment functions this exercises (and that
    // `env_var` calls directly) serialise access internally, so setting a
    // test-unique variable cannot race other tests' lookups.
    unsafe { std::env::set_var("HEGEL_SYS_TEST_EMPTY", "") };
    assert_eq!(env_var("HEGEL_SYS_TEST_EMPTY"), Some(String::new()));
}

#[cfg(windows)]
#[test]
fn env_var_reads_values_longer_than_its_first_buffer() {
    let long = "x".repeat(4000);
    // SAFETY: as in `env_var_distinguishes_empty_from_unset`.
    unsafe { std::env::set_var("HEGEL_SYS_TEST_LONG", &long) };
    assert_eq!(env_var("HEGEL_SYS_TEST_LONG"), Some(long));
}

#[test]
fn stderr_line_is_best_effort() {
    stderr_line("sys_tests: this line goes to the raw stderr fd");
}

#[test]
fn pid_is_nonzero_and_stable() {
    assert_ne!(pid(), 0);
    assert_eq!(pid(), pid());
}
