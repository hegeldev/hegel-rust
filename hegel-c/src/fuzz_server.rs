//! The fuzz server: one process serving many of the fuzzer's test cases
//! by forking at the first draw.
//!
//! `HEGEL_FUZZ_SERVER=<requests>,<replies>` names two named pipes. The
//! process starts the selected test as usual and, when its test case
//! makes the first draw, reads requests from the first pipe instead of
//! drawing, one per line: tab-separated `key=value` fields `output` (the
//! record file, as `HEGEL_FUZZ_OUTPUT`), `prefix` (a choice-sequence file,
//! as `HEGEL_FUZZ_PREFIX`), `seed` (the RNG seed for the draws past the
//! prefix), `tail` (`random` or `none`, as `HEGEL_FUZZ_TAIL`), `misfit`
//! (`random` or `simplest`, as `HEGEL_FUZZ_MISFIT`) and `stderr` (a file
//! the child's standard error goes to). For each request it forks: the
//! child adopts the requested choice source and carries on from that draw
//! exactly as a `HEGEL_FUZZ_OUTPUT` run would, writes its record and
//! exits as soon as its run ends (the frontend sees to that, since a
//! forked child cannot return to a test harness such as libtest); the
//! parent writes `pid <n>` to the reply pipe, waits for the
//! child, writes `exit <code>` or `signal <n>`, and reads the next
//! request. A request it cannot read or serve gets `error <message>` and
//! no child. End of file on the request pipe ends the process. Whatever
//! the test did before its first draw (opening a database, say) is done
//! once and shared by every child.

use alloc::boxed::Box;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use crate::native::core::{ChoiceValue, NativeTestCase};
use crate::native::database::deserialize_choices;
use crate::sys::Error;
use crate::sys::process::Exit;

/// One test case the fuzzer asks the server for.
#[derive(Debug)]
pub(crate) struct Request {
    pub prefix: Vec<ChoiceValue>,
    pub output: String,
    pub stderr: Option<String>,
    pub seed: u64,
    pub exact: bool,
    pub random_misfits: bool,
}

/// A request line decoded, with `read` fetching the prefix file.
pub(crate) fn parse_request(
    line: &str,
    read: &dyn Fn(&str) -> Option<Vec<u8>>,
) -> Result<Request, String> {
    let mut request = Request {
        prefix: Vec::new(),
        output: String::new(),
        stderr: None,
        seed: 0,
        exact: false,
        random_misfits: true,
    };
    for field in line.split('\t').filter(|field| !field.is_empty()) {
        let Some((key, value)) = field.split_once('=') else {
            return Err(format!("field {field:?} is not key=value"));
        };
        match key {
            "prefix" => {
                let Some(bytes) = read(value) else {
                    return Err(format!("prefix {value} could not be read"));
                };
                let Some(choices) = deserialize_choices(&bytes) else {
                    return Err(format!("prefix {value} is not a choice sequence"));
                };
                request.prefix = choices;
            }
            "output" => request.output = value.to_string(),
            "stderr" => request.stderr = Some(value.to_string()),
            "seed" => {
                let Ok(seed) = value.parse() else {
                    return Err(format!("seed {value:?} is not an integer"));
                };
                request.seed = seed;
            }
            "tail" => {
                request.exact = match value {
                    "random" => false,
                    "none" => true,
                    _ => return Err(format!("tail must be random or none, got {value:?}")),
                }
            }
            "misfit" => {
                request.random_misfits = match value {
                    "random" => true,
                    "simplest" => false,
                    _ => {
                        return Err(format!("misfit must be random or simplest, got {value:?}"));
                    }
                }
            }
            _ => return Err(format!("unknown field {key:?}")),
        }
    }
    if request.output.is_empty() {
        return Err("no output field".to_string());
    }
    Ok(request)
}

/// The process control the server loop needs, so that the loop can be
/// driven without forking.
pub(crate) trait Process {
    fn fork(&mut self) -> Result<Option<u32>, Error>;
    fn wait(&mut self, pid: u32) -> Result<Exit, Error>;
    fn redirect_stderr(&mut self, path: &str);
}

/// The real thing.
pub(crate) struct OsProcess;

impl Process for OsProcess {
    fn fork(&mut self) -> Result<Option<u32>, Error> {
        crate::sys::process::fork()
    }

    fn wait(&mut self, pid: u32) -> Result<Exit, Error> {
        crate::sys::process::wait(pid)
    }

    fn redirect_stderr(&mut self, path: &str) {
        let _ = crate::sys::process::redirect_stderr(path);
    }
}

/// How [`serve`] ended: as the child that answers a request, with the
/// choice source it asked for, or as the parent once the requests ran
/// out.
pub(crate) enum Served {
    Child(Request, Box<NativeTestCase>),
    Finished,
}

/// Serve requests until they run out, forking a child for each one that
/// `build` can make a choice source for.
pub(crate) fn serve(
    requests: &mut dyn FnMut() -> Result<Option<String>, Error>,
    reply: &mut dyn FnMut(&str),
    read: &dyn Fn(&str) -> Option<Vec<u8>>,
    build: &dyn Fn(&Request) -> Result<NativeTestCase, String>,
    process: &mut dyn Process,
) -> Served {
    loop {
        let Ok(Some(line)) = requests() else {
            return Served::Finished;
        };
        let source = parse_request(&line, read).and_then(|request| {
            let source = build(&request)?;
            Ok((request, source))
        });
        let (request, source) = match source {
            Ok(served) => served,
            Err(message) => {
                reply(&format!("error {message}\n"));
                continue;
            }
        };
        match process.fork() {
            Err(_) => reply("error fork failed\n"),
            Ok(None) => {
                if let Some(path) = &request.stderr {
                    process.redirect_stderr(path);
                }
                return Served::Child(request, Box::new(source));
            }
            Ok(Some(pid)) => {
                reply(&format!("pid {pid}\n"));
                let ended = match process.wait(pid) {
                    Ok(Exit::Code(code)) => format!("exit {code}\n"),
                    Ok(Exit::Signal(signal)) => format!("signal {signal}\n"),
                    Err(_) => "error wait failed\n".to_string(),
                };
                reply(&ended);
            }
        }
    }
}

#[cfg(test)]
#[path = "../tests/embedded/fuzz_server_tests.rs"]
mod tests;
