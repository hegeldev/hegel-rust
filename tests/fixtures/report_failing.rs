//! Fixture binary: a failing run at a verbosity chosen by
//! `HEGEL_FIXTURE_VERBOSITY` (`quiet`, `normal`, `verbose`, or `debug`).
//! The reporting tests assert which progress/report lines each verbosity
//! level puts on stderr, which requires a real process: the engine's
//! progress output (`Running test case`, `Test done.`) is written straight
//! to stderr, not through hegel's capturable output sink.

use hegel::generators as gs;
use hegel::{Hegel, Settings, Verbosity};

fn main() {
    let verbosity = match std::env::var("HEGEL_FIXTURE_VERBOSITY").as_deref() {
        Ok("quiet") => Verbosity::Quiet,
        Ok("verbose") => Verbosity::Verbose,
        Ok("debug") => Verbosity::Debug,
        Ok("normal") | Err(_) => Verbosity::Normal,
        Ok(other) => panic!("unknown HEGEL_FIXTURE_VERBOSITY {other:?}"),
    };
    Hegel::new(|tc| {
        let i: i64 = tc.draw(gs::integers::<i64>());
        assert!(i < 10);
    })
    .settings(
        Settings::new()
            .verbosity(verbosity)
            .test_cases(1000)
            .database(None),
    )
    .run();
}
