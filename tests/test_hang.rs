mod common;

use hegel::generators as gs;
use hegel::{HealthCheck, TestCase};

#[hegel::test(suppress_health_check = HealthCheck::all())]
fn test_does_not_hang_on_assume_false(tc: TestCase) {
    println!("Running...");
    tc.draw(gs::integers::<i32>());
    tc.assume(false);
}
