//! End-to-end tests for the fuzzer client — the `HEGEL_FUZZ_*` environment
//! variables an external fuzzer drives a test program with — against the
//! prebuilt `#[hegel::main]` fixture binaries.

mod common;

use common::exec::fixture;

const BASIC_MAIN: &str = env!("CARGO_BIN_EXE_fixture_basic_main");
const MAIN_FAILING: &str = env!("CARGO_BIN_EXE_fixture_main_failing");

/// A one-integer choice sequence in the failure database's entry format:
/// the choice count, the integer tag, the big-integer sub-tag, the byte
/// length and the integer's two's-complement little-endian bytes.
fn integer_entry(value: &[u8]) -> Vec<u8> {
    let mut entry = Vec::new();
    entry.extend_from_slice(&1u32.to_le_bytes());
    entry.push(0);
    entry.push(10);
    entry.extend_from_slice(&(value.len() as u32).to_le_bytes());
    entry.extend_from_slice(value);
    entry
}

fn read_record(path: &std::path::Path) -> serde_json::Value {
    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
}

#[test]
fn fuzz_output_runs_one_case_and_records_it() {
    let dir = tempfile::TempDir::new().unwrap();
    let out = dir.path().join("record.json");
    let output = fixture(BASIC_MAIN)
        .env("HEGEL_FUZZ_OUTPUT", out.to_str().unwrap())
        .run();
    assert_eq!(output.stderr.matches("ran").count(), 1);
    let record = read_record(&out);
    assert_eq!(record["status"], "valid");
    assert_eq!(record["test"], "fixture_basic_main::main");
    assert_eq!(record["origin"], serde_json::Value::Null);
    assert_eq!(record["prefix_length"], 0);
    assert_eq!(record["choices"].as_array().unwrap().len(), 1);
    assert_eq!(record["choices"][0]["kind"], "integer");
    assert!(!record["spans"].as_array().unwrap().is_empty());
    assert!(record["choices_base64"].is_string());
}

#[test]
fn fuzz_trace_streams_the_choices_without_the_count() {
    let dir = tempfile::TempDir::new().unwrap();
    let out = dir.path().join("record.json");
    let trace = dir.path().join("trace");
    fixture(BASIC_MAIN)
        .env("HEGEL_FUZZ_OUTPUT", out.to_str().unwrap())
        .env("HEGEL_FUZZ_TRACE", trace.to_str().unwrap())
        .run();
    let record = read_record(&out);
    let entry = base64_decode(record["choices_base64"].as_str().unwrap());
    assert_eq!(std::fs::read(&trace).unwrap(), entry[4..]);
}

#[test]
fn fuzz_output_records_a_failure_and_the_run_fails_as_usual() {
    let dir = tempfile::TempDir::new().unwrap();
    let out = dir.path().join("record.json");
    let output = fixture(MAIN_FAILING)
        .env("HEGEL_FUZZ_OUTPUT", out.to_str().unwrap())
        .expect_failure("got nonneg")
        .run();
    assert_eq!(
        output.stderr.matches("got nonneg").count(),
        1,
        "fuzz mode runs the body exactly once, with no replays or shrinking: {}",
        output.stderr
    );
    assert!(output.stderr.contains("reproduce_failure"));
    let record = read_record(&out);
    assert_eq!(record["status"], "interesting");
    assert!(
        record["origin"].as_str().unwrap().starts_with("Panic at "),
        "{record}"
    );
}

#[test]
fn fuzz_prefix_replays_the_stored_choices() {
    let dir = tempfile::TempDir::new().unwrap();
    let prefix = dir.path().join("prefix");
    std::fs::write(&prefix, integer_entry(&(-1234i16).to_le_bytes())).unwrap();
    let out = dir.path().join("record.json");
    fixture(BASIC_MAIN)
        .env("HEGEL_FUZZ_PREFIX", prefix.to_str().unwrap())
        .env("HEGEL_FUZZ_OUTPUT", out.to_str().unwrap())
        .run();
    let record = read_record(&out);
    assert_eq!(record["choices"][0]["value"], "-1234");
    assert_eq!(record["prefix_length"], 1);
    assert_eq!(record["prefix_consumed"], 1);
    assert_eq!(record["misaligned_at"], serde_json::Value::Null);
    let entry = base64_decode(record["choices_base64"].as_str().unwrap());
    assert_eq!(entry, integer_entry(&(-1234i16).to_le_bytes()));
}

#[test]
fn fuzz_test_selects_the_test_the_variables_are_for() {
    let dir = tempfile::TempDir::new().unwrap();
    let out = dir.path().join("record.json");
    let output = fixture(BASIC_MAIN)
        .env("HEGEL_FUZZ_OUTPUT", out.to_str().unwrap())
        .env("HEGEL_FUZZ_TEST", "some_other_crate::main")
        .run();
    assert_eq!(output.stderr.matches("ran").count(), 0);
    assert!(!out.exists());

    let output = fixture(BASIC_MAIN)
        .env("HEGEL_FUZZ_OUTPUT", out.to_str().unwrap())
        .env("HEGEL_FUZZ_TEST", "fixture_basic_main::main")
        .run();
    assert_eq!(output.stderr.matches("ran").count(), 1);
    assert!(out.exists());
}

#[test]
fn fuzz_reproduce_shrinks_and_persists_the_failure() {
    let dir = tempfile::TempDir::new().unwrap();
    let prefix = dir.path().join("prefix");
    std::fs::write(&prefix, integer_entry(&[37])).unwrap();
    let db = dir.path().join("db");
    let output = fixture(MAIN_FAILING)
        .env("HEGEL_FUZZ_REPRODUCE", prefix.to_str().unwrap())
        .args(&["--database", db.to_str().unwrap()])
        .expect_failure("got nonneg 0")
        .run();
    assert!(db.is_dir(), "{}", output.stderr);

    let output = fixture(MAIN_FAILING)
        .args(&["--database", db.to_str().unwrap(), "--verbosity", "debug"])
        .expect_failure("got nonneg 0")
        .run();
    assert!(
        output.stderr.contains("Starting phase: Reuse"),
        "{}",
        output.stderr
    );
}

#[test]
fn fuzz_reproduce_of_a_passing_entry_runs_it_once_and_generates_nothing() {
    let dir = tempfile::TempDir::new().unwrap();
    let prefix = dir.path().join("prefix");
    std::fs::write(&prefix, integer_entry(&[5])).unwrap();
    let output = fixture(BASIC_MAIN)
        .env("HEGEL_FUZZ_REPRODUCE", prefix.to_str().unwrap())
        .run();
    assert_eq!(output.stderr.matches("ran").count(), 1);
}

#[test]
fn a_malformed_fuzz_variable_is_a_run_error() {
    let dir = tempfile::TempDir::new().unwrap();
    let out = dir.path().join("record.json");
    fixture(BASIC_MAIN)
        .env("HEGEL_FUZZ_OUTPUT", out.to_str().unwrap())
        .env(
            "HEGEL_FUZZ_PREFIX",
            dir.path().join("missing").to_str().unwrap(),
        )
        .expect_failure("HEGEL_FUZZ_PREFIX=.* could not be read")
        .run();
    assert!(!out.exists());
}

fn base64_decode(text: &str) -> Vec<u8> {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut bits: u32 = 0;
    let mut count = 0;
    let mut out = Vec::new();
    for byte in text.bytes().filter(|&b| b != b'=') {
        let value = ALPHABET.iter().position(|&a| a == byte).unwrap() as u32;
        bits = (bits << 6) | value;
        count += 6;
        if count >= 8 {
            count -= 8;
            out.push((bits >> count) as u8);
            bits &= (1 << count) - 1;
        }
    }
    out
}

#[test]
fn a_compact_record_has_the_realized_form_but_no_choice_or_span_arrays() {
    let dir = tempfile::TempDir::new().unwrap();
    let out = dir.path().join("record.json");
    fixture(BASIC_MAIN)
        .env("HEGEL_FUZZ_OUTPUT", out.to_str().unwrap())
        .env("HEGEL_FUZZ_RECORD", "compact")
        .run();
    let record = read_record(&out);
    assert!(record.get("choices").is_none());
    assert!(record.get("spans").is_none());
    assert!(record["realized_base64"].is_string());
    assert!(record["choices_base64"].is_string());
    assert_eq!(record["status"], "valid");
}

#[test]
fn fuzz_coverage_writes_the_case_s_counter_map_or_refuses_an_uninstrumented_program() {
    let dir = tempfile::TempDir::new().unwrap();
    let out = dir.path().join("record.json");
    let map = dir.path().join("coverage.map");
    let command = fixture(BASIC_MAIN)
        .env("HEGEL_FUZZ_OUTPUT", out.to_str().unwrap())
        .env("HEGEL_FUZZ_COVERAGE", map.to_str().unwrap());
    if cfg!(hegel_coverage) {
        command.run();
        assert_eq!(read_record(&out)["status"], "valid");
        let bytes = std::fs::read(&map).unwrap();
        assert!(bytes.iter().all(|&b| b <= 8));
        assert!(bytes.iter().any(|&b| b > 0));
    } else {
        command
            .expect_failure("HEGEL_FUZZ_COVERAGE=.* not built with -C instrument-coverage")
            .run();
        assert!(!map.exists());
    }
}

#[cfg(unix)]
mod server {
    use super::{BASIC_MAIN, MAIN_FAILING, integer_entry, read_record};
    use std::io::{BufRead, BufReader, Write};
    use std::process::{Command, Stdio};

    const MAIN_NODRAW: &str = env!("CARGO_BIN_EXE_fixture_main_nodraw");
    const MAIN_SLOW: &str = env!("CARGO_BIN_EXE_fixture_main_slow");

    /// A fuzz server over two named pipes in `dir`. The request pipe is
    /// held open for reading and writing so the server never sees its end
    /// early; the reply pipe is read without blocking, so a server that
    /// dies before replying fails the test instead of hanging it.
    struct Server {
        child: std::process::Child,
        requests: std::fs::File,
        replies: BufReader<std::fs::File>,
    }

    #[cfg(target_os = "macos")]
    const O_NONBLOCK: i32 = 0x4;
    #[cfg(not(target_os = "macos"))]
    const O_NONBLOCK: i32 = 0x800;

    fn start(exe: &str, dir: &std::path::Path) -> Server {
        let requests = dir.join("requests");
        let replies = dir.join("replies");
        for pipe in [&requests, &replies] {
            assert!(Command::new("mkfifo").arg(pipe).status().unwrap().success());
        }
        let mut command = Command::new(exe);
        if cfg!(hegel_coverage) {
            command.env("HEGEL_FUZZ_COVERAGE", dir.join("coverage.map"));
        }
        let child = command
            .current_dir(dir)
            .env(
                "HEGEL_FUZZ_SERVER",
                format!("{},{}", requests.display(), replies.display()),
            )
            .env("HEGEL_DATABASE", "disabled")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        use std::os::unix::fs::OpenOptionsExt;
        let requests = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&requests)
            .unwrap();
        let replies = std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(O_NONBLOCK)
            .open(&replies)
            .unwrap();
        Server {
            child,
            requests,
            replies: BufReader::new(replies),
        }
    }

    impl Server {
        fn reply(&mut self) -> String {
            let mut line = String::new();
            loop {
                match self.replies.read_line(&mut line) {
                    Ok(0) | Err(_) if line.ends_with('\n') => break,
                    Ok(n) if n > 0 && line.ends_with('\n') => break,
                    Err(e) if e.kind() != std::io::ErrorKind::WouldBlock => panic!("{e}"),
                    _ => {}
                }
                if let Some(status) = self.child.try_wait().unwrap() {
                    panic!("server exited with {status} before replying {line:?}");
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            line.trim_end().to_string()
        }

        fn request(&mut self, line: &str) -> (String, String) {
            writeln!(self.requests, "{line}").unwrap();
            let pid = self.reply();
            assert!(pid.starts_with("pid "), "{pid}");
            (pid, self.reply())
        }

        fn finish(self) -> std::process::Output {
            drop(self.requests);
            drop(self.replies);
            let output = self.child.wait_with_output().unwrap();
            assert!(output.status.success(), "{output:?}");
            output
        }
    }

    #[test]
    fn a_server_runs_one_case_per_request_and_ends_with_the_requests() {
        let dir = tempfile::TempDir::new().unwrap();
        let mut server = start(BASIC_MAIN, dir.path());
        let out1 = dir.path().join("1.json");
        let out2 = dir.path().join("2.json");
        let prefix = dir.path().join("prefix");
        std::fs::write(&prefix, integer_entry(&[7])).unwrap();
        let (_, ended) = server.request(&format!("output={}\tseed=1", out1.display()));
        assert_eq!(ended, "exit 0");
        if cfg!(hegel_coverage) {
            let map = std::fs::read(dir.path().join("coverage.map")).unwrap();
            assert!(map.iter().any(|&b| b > 0));
        }
        let (_, ended) = server.request(&format!(
            "output={}\tprefix={}\ttail=none\tmisfit=simplest",
            out2.display(),
            prefix.display()
        ));
        assert_eq!(ended, "exit 0");
        let out3 = dir.path().join("3.json");
        let (_, ended) = server.request(&format!(
            "output={}\tprefix={}\tmisfit=simplest\tseed=2",
            out3.display(),
            prefix.display()
        ));
        assert_eq!(ended, "exit 0");
        assert_eq!(read_record(&out3)["choices"][0]["value"], "7");
        let first = read_record(&out1);
        assert_eq!(first["status"], "valid");
        assert_eq!(first["prefix_length"], 0);
        let second = read_record(&out2);
        assert_eq!(second["status"], "valid");
        assert_eq!(second["prefix_length"], 1);
        assert_eq!(second["choices"][0]["value"], "7");
        server.finish();
    }

    #[test]
    fn a_failing_case_is_the_child_s_failure_with_its_own_stderr() {
        let dir = tempfile::TempDir::new().unwrap();
        let mut server = start(MAIN_FAILING, dir.path());
        let out = dir.path().join("out.json");
        let err = dir.path().join("err.txt");
        let (_, ended) = server.request(&format!(
            "output={}\tstderr={}\tseed=3",
            out.display(),
            err.display()
        ));
        assert_eq!(ended, "exit 101");
        assert_eq!(read_record(&out)["status"], "interesting");
        let stderr = std::fs::read_to_string(&err).unwrap();
        assert!(stderr.contains("panicked"), "{stderr}");
        writeln!(server.requests, "nonsense").unwrap();
        let ended = server.reply();
        assert!(ended.starts_with("error "), "{ended}");
        let output = server.finish();
        assert!(!String::from_utf8_lossy(&output.stderr).contains("panicked"));
    }

    #[test]
    fn a_case_killed_by_a_signal_is_reported_as_that_signal() {
        let dir = tempfile::TempDir::new().unwrap();
        let mut server = start(MAIN_SLOW, dir.path());
        let slow = dir.path().join("slow.json");
        writeln!(server.requests, "output={}\tseed=1", slow.display()).unwrap();
        let pid = server.reply();
        let pid = pid.strip_prefix("pid ").unwrap();
        assert!(
            Command::new("kill")
                .args(["-9", pid])
                .status()
                .unwrap()
                .success()
        );
        assert_eq!(server.reply(), "signal 9");
        assert!(!slow.exists());
        server.finish();
    }

    #[test]
    fn unopenable_pipes_end_the_server() {
        let dir = tempfile::TempDir::new().unwrap();
        let output = Command::new(BASIC_MAIN)
            .current_dir(dir.path())
            .env(
                "HEGEL_FUZZ_SERVER",
                format!(
                    "{},{}",
                    dir.path().join("missing").join("requests").display(),
                    dir.path().join("missing").join("replies").display()
                ),
            )
            .env("HEGEL_DATABASE", "disabled")
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(2));
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains("could not open"), "{stderr}");
    }

    #[test]
    fn a_test_that_draws_nothing_cannot_serve() {
        let dir = tempfile::TempDir::new().unwrap();
        let server = start(MAIN_NODRAW, dir.path());
        let output = server.child.wait_with_output().unwrap();
        assert!(!output.status.success());
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains("drew nothing"), "{stderr}");
    }
}
