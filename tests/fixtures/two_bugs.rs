//! Fixture binary: a property with two distinct panic sites and
//! `report_multiple_failures(true)`, for asserting the multi-failure
//! report's stderr layout (headline first, then one self-contained
//! draws-plus-diagnostic block per failure).

use hegel::generators as gs;
use hegel::{Hegel, Settings};

fn main() {
    Hegel::new(|tc| {
        let x: i32 = tc.draw(gs::integers::<i32>().min_value(0).max_value(40));
        if x > 30 {
            panic!("big branch: {}", x);
        }
        if x < 10 {
            panic!("small branch: {}", x);
        }
    })
    .settings(
        Settings::new()
            .database(None)
            .derandomize(true)
            .test_cases(500)
            .report_multiple_failures(true),
    )
    .run();
}
