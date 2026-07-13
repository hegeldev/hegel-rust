//! Fixture binary: every test case rejects (`assume`) on a *worker* thread.
//! Control-flow unwinds must be invisible on stderr no matter which thread
//! raises them — the driver asserts no `panicked` noise appears.

use hegel::generators as gs;
use hegel::{HealthCheck, Hegel, Settings};

fn main() {
    Hegel::new(|tc| {
        let worker = tc.clone();
        let result = std::thread::spawn(move || {
            let keep: bool = worker.draw(gs::booleans());
            // Rejecting on the worker thread raises the control-flow unwind
            // *there*, not on the thread the lifecycle's hook protects.
            worker.assume(keep);
        })
        .join();
        if let Err(payload) = result {
            std::panic::resume_unwind(payload);
        }
    })
    .settings(
        Settings::new()
            .database(None)
            .derandomize(true)
            .test_cases(20)
            .suppress_health_check(HealthCheck::all()),
    )
    .run();
}
