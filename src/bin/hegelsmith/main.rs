mod gen_expr;
mod program;
mod statements;
mod types;

use hegel::{Hegel, Settings, TestCase};
use program::generate_program;
use std::io::Write;
use std::process::{Command, Stdio};

fn main() {
    let args: Vec<String> = std::env::args().collect();

    if args.len() < 2 {
        eprintln!("Usage: hegelsmith <executable> [--test-cases N] [--seed S] [--print-only]");
        std::process::exit(1);
    }

    let mut executable = None;
    let mut test_cases = 100u64;
    let mut seed = None;
    let mut print_only = false;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--test-cases" => {
                i += 1;
                test_cases = args[i].parse().unwrap();
            }
            "--seed" => {
                i += 1;
                seed = Some(args[i].parse::<u64>().unwrap());
            }
            "--print-only" => {
                print_only = true;
            }
            other => {
                executable = Some(other.to_string());
            }
        }
        i += 1;
    }

    if print_only {
        Hegel::new(|tc: TestCase| {
            let program = generate_program(&tc);
            println!("{program}");
        })
        .settings({
            let mut s = Settings::new().test_cases(test_cases);
            if let Some(seed_val) = seed {
                s = s.seed(Some(seed_val));
            }
            s
        })
        .run();
        return;
    }

    let executable = executable.unwrap_or_else(|| {
        eprintln!("Error: executable argument required (unless --print-only)");
        std::process::exit(1);
    });

    let mut settings = Settings::new().test_cases(test_cases);
    if let Some(s) = seed {
        settings = settings.seed(Some(s));
    }

    Hegel::new(move |tc: TestCase| {
        let program = generate_program(&tc);

        let mut child = Command::new(&executable)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();

        child
            .stdin
            .as_mut()
            .unwrap()
            .write_all(program.as_bytes())
            .unwrap();
        drop(child.stdin.take());

        let output = child.wait_with_output().unwrap();
        assert!(
            output.status.success(),
            "Program failed (exit code {:?}):\n{}\n--- stderr ---\n{}",
            output.status.code(),
            program,
            String::from_utf8_lossy(&output.stderr)
        );
    })
    .settings(settings)
    .run();
}
