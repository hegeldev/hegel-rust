use hegel::gen::{self, Generate};

#[test]
fn test_default_runs_100_test_cases() {
    let mut count = 0;

    hegel::hegel(|| {
        let _ = gen::integers::<i32>().generate();
        count += 1;
    });

    assert_eq!(count, 100);
}
