use hegel::generators as gs;
use hegel::stateful::run_concurrent;
use hegel::{Hegel, Settings, TestCase};
use std::sync::atomic::{AtomicI64, Ordering};
use std::time::Instant;

struct RacyCounter {
    value: AtomicI64,
    increments: AtomicI64,
}

#[hegel::concurrent_state_machine]
impl RacyCounter {
    #[rule]
    fn racy_increment(&self, _: TestCase) {
        let value = self.value.load(Ordering::SeqCst);
        std::thread::yield_now();
        self.value.store(value + 1, Ordering::SeqCst);
        self.increments.fetch_add(1, Ordering::SeqCst);
    }

    #[invariant]
    fn no_lost_updates(&self, _: TestCase) {
        assert_eq!(
            self.value.load(Ordering::SeqCst),
            self.increments.load(Ordering::SeqCst)
        );
    }
}

fn racy_body(tc: TestCase) {
    let m = RacyCounter {
        value: AtomicI64::new(0),
        increments: AtomicI64::new(0),
    };
    run_concurrent(m, tc, 2, 4);
}

static CLONE_CALLS: AtomicI64 = AtomicI64::new(0);

fn clone_flaky_body(tc: TestCase) {
    let child = tc.clone();
    let x: i64 = child.draw(gs::integers::<i64>().min_value(0).max_value(1000));
    let call = CLONE_CALLS.fetch_add(1, Ordering::SeqCst);
    if call % 3 == 0 {
        assert!(x < 500, "clone-flaky: x = {x}");
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mode = args[1].clone();
    let arg = args.get(2).cloned();
    let start = Instant::now();
    let outcome = std::panic::catch_unwind(move || {
        let body: fn(TestCase) = if mode.contains("racy") {
            racy_body
        } else {
            clone_flaky_body
        };
        let h = Hegel::new(body);
        match mode.split('-').next().unwrap() {
            "discover" | "reuse" => h
                .settings(
                    Settings::new()
                        .database(Some(arg.unwrap()))
                        .test_cases(100)
                        .print_blob(true),
                )
                .run(),
            "replay" => h
                .settings(Settings::new().database(None))
                .reproduce_failure(arg.unwrap())
                .run(),
            other => panic!("unknown mode {other}"),
        }
    });
    let elapsed = start.elapsed().as_secs_f64();
    println!("EXP007-SECONDS: {elapsed:.2}");
    match outcome {
        Ok(()) => println!("EXP007-RESULT: PASSED"),
        Err(payload) => {
            let msg = payload
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| payload.downcast_ref::<&str>().map(|s| s.to_string()))
                .unwrap_or_default();
            println!("EXP007-PANIC: {}", msg.replace('\n', " / "));
            println!("EXP007-RESULT: FAILED");
        }
    }
}
