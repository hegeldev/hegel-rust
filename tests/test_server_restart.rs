#![cfg(not(feature = "native"))]
mod common;

/// After a server crash, Hegel should transparently restart the server for the
/// next test run rather than propagating the crash to unrelated tests.
#[test]
fn test_server_restarts_after_kill() {
    // First run — starts the server and completes successfully.
    hegel::Hegel::new(|tc| {
        let _ = tc.draw(hegel::generators::booleans());
    })
    .settings(hegel::Settings::new().test_cases(1))
    .run();

    // Kill the server and wait for the connection to detect it has exited.
    hegel::__test_kill_server();

    // Second run — should detect the dead session, restart the server, and succeed.
    hegel::Hegel::new(|tc| {
        let _ = tc.draw(hegel::generators::booleans());
    })
    .settings(hegel::Settings::new().test_cases(1))
    .run();
}
