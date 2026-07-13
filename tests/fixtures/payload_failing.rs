//! Fixture binary: a failing run whose closing re-raise must skip the panic
//! hook, so the failure message appears exactly once on stderr (in the
//! diagnostic printed at the catch site).

use hegel::generators as gs;
use hegel::{Hegel, Settings};

fn main() {
    Hegel::new(|tc| {
        let _: bool = tc.draw(gs::booleans());
        panic!("intentional failure");
    })
    .settings(Settings::new().database(None).derandomize(true))
    .run();
}
