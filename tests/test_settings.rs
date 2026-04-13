use hegel::generators as gs;

#[test]
fn test_default_runs_100_test_cases() {
    let mut count = 0;

    hegel::hegel(|tc| {
        let _ = tc.draw(gs::integers::<i32>());
        count += 1;
    });

    assert_eq!(count, 100);
}

#[test]
fn test_settings_default_trait() {
    let settings = hegel::Settings::default();
    let mut count = 0;

    hegel::Hegel::new(|tc| {
        let _ = tc.draw(gs::integers::<i32>());
        count += 1;
    })
    .settings(settings)
    .run();

    assert_eq!(count, 100);
}

#[test]
fn test_settings_verbosity() {
    let mut count = 0;

    hegel::Hegel::new(|tc| {
        let _ = tc.draw(gs::integers::<i32>());
        count += 1;
    })
    .settings(
        hegel::Settings::new()
            .verbosity(hegel::Verbosity::Quiet)
            .test_cases(10),
    )
    .run();

    assert_eq!(count, 10);
}

#[test]
fn test_settings_verbosity_debug() {
    // Exercises the debug-mode eprintln paths in ServerDataSource::send_request
    // and ServerTestRunner::run (REQUEST, RESPONSE, run_test response, events, test done).
    let mut count = 0;

    hegel::Hegel::new(|tc| {
        let _ = tc.draw(gs::booleans());
        count += 1;
    })
    .settings(
        hegel::Settings::new()
            .verbosity(hegel::Verbosity::Debug)
            .test_cases(1),
    )
    .run();

    assert_eq!(count, 1);
}
