use hegel::gen::{self, Generate};

#[test]
fn test_integers_i32_within_bounds() {
    hegel::hegel(|| {
        let x = gen::integers::<i32>().generate();
        assert!(x >= i32::MIN && x <= i32::MAX);
    })
}
