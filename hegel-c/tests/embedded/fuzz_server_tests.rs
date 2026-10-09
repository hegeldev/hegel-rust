use super::*;
use crate::native::bignum::BigInt;
use crate::native::database::serialize_choices;
use alloc::vec;
use std::cell::RefCell;

fn int(v: i64) -> ChoiceValue {
    ChoiceValue::Integer(BigInt::from(v))
}

fn prefix_file() -> (String, Vec<u8>) {
    let bytes = serialize_choices(&[int(3), ChoiceValue::Boolean(true)]).unwrap();
    ("/prefix".to_string(), bytes)
}

#[test]
fn a_request_line_decodes_every_field() {
    let (path, bytes) = prefix_file();
    let read = |p: &str| (p == path).then(|| bytes.clone());
    let line = format!(
        "output=/out.json\tprefix={path}\tseed=42\ttail=none\tmisfit=simplest\tstderr=/err.txt\t"
    );
    let request = parse_request(&line, &read).unwrap();
    assert_eq!(request.prefix, vec![int(3), ChoiceValue::Boolean(true)]);
    assert_eq!(request.output, "/out.json");
    assert_eq!(request.stderr.as_deref(), Some("/err.txt"));
    assert_eq!(request.seed, 42);
    assert!(request.exact);
    assert!(!request.random_misfits);
    let request = parse_request("output=o\ttail=random\tmisfit=random", &read).unwrap();
    assert!(request.prefix.is_empty());
    assert!(!request.exact);
    assert!(request.random_misfits);
    assert_eq!(request.seed, 0);
}

#[test]
fn a_malformed_request_names_what_is_wrong() {
    let read = |p: &str| (p == "/ok").then(|| vec![0u8; 3]);
    let bad = |line: &str| parse_request(line, &read).unwrap_err();
    assert!(bad("output").contains("key=value"));
    assert!(bad("output=o\tprefix=/missing").contains("could not be read"));
    assert!(bad("output=o\tprefix=/ok").contains("not a choice sequence"));
    assert!(bad("output=o\tseed=x").contains("seed"));
    assert!(bad("output=o\ttail=maybe").contains("tail"));
    assert!(bad("output=o\tmisfit=maybe").contains("misfit"));
    assert!(bad("output=o\tcolour=red").contains("unknown field"));
    assert!(bad("seed=1").contains("no output"));
}

/// A process that forks into a parent for every request but the one
/// numbered `child_on`, where it is the child.
struct FakeProcess {
    forks: u32,
    child_on: u32,
    fork_fails: bool,
    exits: Vec<Result<Exit, Error>>,
    redirected: Vec<String>,
}

impl Process for FakeProcess {
    fn fork(&mut self) -> Result<Option<u32>, Error> {
        self.forks += 1;
        if self.fork_fails {
            return Err(Error);
        }
        Ok((self.forks != self.child_on).then_some(100 + self.forks))
    }

    fn wait(&mut self, _pid: u32) -> Result<Exit, Error> {
        self.exits.remove(0)
    }

    fn redirect_stderr(&mut self, path: &str) {
        self.redirected.push(path.to_string());
    }
}

fn run_server(
    lines: &[&str],
    process: &mut FakeProcess,
    build: &dyn Fn(&Request) -> Result<NativeTestCase, String>,
) -> (Served, Vec<String>) {
    let queue = RefCell::new(lines.iter().map(|l| l.to_string()).collect::<Vec<_>>());
    let replies = RefCell::new(Vec::new());
    let served = serve(
        &mut || {
            let mut queue = queue.borrow_mut();
            if queue.is_empty() {
                Ok(None)
            } else {
                Ok(Some(queue.remove(0)))
            }
        },
        &mut |line| replies.borrow_mut().push(line.to_string()),
        &|_| None,
        build,
        process,
    );
    (served, replies.into_inner())
}

fn build(request: &Request) -> Result<NativeTestCase, String> {
    crate::native::test_runner::fuzz_source(
        &request.prefix,
        request.random_misfits,
        request.exact,
        crate::native::rng::EngineRng::seeded(request.seed),
        100,
    )
    .map_err(|e| format!("{e:?}"))
}

#[test]
fn the_parent_reports_each_child_and_finishes_when_requests_end() {
    let mut process = FakeProcess {
        forks: 0,
        child_on: 0,
        fork_fails: false,
        exits: vec![Ok(Exit::Code(0)), Ok(Exit::Signal(9)), Err(Error)],
        redirected: Vec::new(),
    };
    let (served, replies) = run_server(
        &["output=a", "output=b", "output=c", "nonsense"],
        &mut process,
        &build,
    );
    assert!(matches!(served, Served::Finished));
    assert_eq!(
        replies,
        vec![
            "pid 101\n",
            "exit 0\n",
            "pid 102\n",
            "signal 9\n",
            "pid 103\n",
            "error wait failed\n",
            "error field \"nonsense\" is not key=value\n",
        ]
    );
    assert!(process.redirected.is_empty());
}

#[test]
fn the_child_adopts_its_request_and_its_stderr() {
    let mut process = FakeProcess {
        forks: 0,
        child_on: 2,
        fork_fails: false,
        exits: vec![Ok(Exit::Code(1))],
        redirected: Vec::new(),
    };
    let (served, replies) = run_server(
        &["output=a", "output=b\tstderr=/err\tseed=7", "output=c"],
        &mut process,
        &build,
    );
    let Served::Child(request, _) = served else {
        panic!("expected the child");
    };
    assert_eq!(request.output, "b");
    assert_eq!(request.seed, 7);
    assert_eq!(replies, vec!["pid 101\n", "exit 1\n"]);
    assert_eq!(process.redirected, vec!["/err"]);
}

#[test]
fn a_failed_fork_or_source_is_reported_and_serving_goes_on() {
    let mut process = FakeProcess {
        forks: 0,
        child_on: 0,
        fork_fails: true,
        exits: Vec::new(),
        redirected: Vec::new(),
    };
    let (served, replies) = run_server(&["output=a", "output=b"], &mut process, &|request| {
        if request.output == "a" {
            Err("no source".to_string())
        } else {
            build(request)
        }
    });
    assert!(matches!(served, Served::Finished));
    assert_eq!(replies, vec!["error no source\n", "error fork failed\n"]);
}

#[test]
fn a_failing_request_read_finishes_serving() {
    let mut process = FakeProcess {
        forks: 0,
        child_on: 0,
        fork_fails: false,
        exits: Vec::new(),
        redirected: Vec::new(),
    };
    let served = serve(
        &mut || Err(Error),
        &mut |_| {},
        &|_| None,
        &build,
        &mut process,
    );
    assert!(matches!(served, Served::Finished));
    assert_eq!(process.forks, 0);
}

#[test]
fn the_os_process_forks_waits_and_redirects() {
    let dir = tempfile::TempDir::new().unwrap();
    let err = dir.path().join("err.txt").to_str().unwrap().to_string();
    let mut process = OsProcess;
    match process.fork().unwrap() {
        None => {
            process.redirect_stderr(&err);
            crate::sys::process::exit(7);
        }
        Some(pid) => {
            assert_eq!(process.wait(pid).unwrap(), Exit::Code(7));
        }
    }
    assert!(crate::sys::fs::exists(&err));
    assert!(process.wait(1).is_err());
}
