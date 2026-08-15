use hegel::generators as gs;
use hegel::prelude::*;

#[composite]
fn even_integers(tc: &TestCase) -> i32 {
    tc.draw(gs::integers::<i32>().map(|n: i32| n.wrapping_mul(2)))
}

#[hegel::test]
fn test_prelude_supplies_test_case_composite_and_generator_trait(tc: TestCase) {
    let xs = tc.draw(gs::vecs(even_integers()));
    let total = xs.iter().fold(0i32, |acc, n| acc.wrapping_add(*n));
    assert_eq!(total % 2, 0);
}

mod generators_module {
    use hegel::prelude::generators as gs;
    use hegel::prelude::*;

    #[hegel::test]
    fn test_prelude_provides_generators_module(tc: TestCase) {
        let n = tc.draw(gs::integers::<i32>().filter(|n: &i32| n % 2 == 0));
        assert_eq!(n % 2, 0);
    }
}

mod standard_test_attribute {
    use hegel::prelude::*;

    #[test]
    fn test_prelude_does_not_shadow_the_standard_test_attribute() {
        assert_eq!(
            std::any::type_name::<TestCase>(),
            std::any::type_name::<hegel::TestCase>()
        );
    }
}
