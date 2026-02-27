use hegel::generators;

#[test]
fn test_default_runs_100_test_cases() {
    let mut count = 0;

    hegel::hegel(|| {
        let _ = hegel::draw(&generators::integers::<i32>());
        count += 1;
    });

    assert_eq!(count, 100);
}
