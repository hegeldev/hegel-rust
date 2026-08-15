use hegel::prelude::*;

#[composite]
fn even_integers(tc: &TestCase) -> i32 {
    tc.draw(gs::integers::<i32>().map(|n: i32| n.wrapping_mul(2)))
}

#[hegel::test]
fn test_prelude_alone_supplies_a_whole_test(tc: TestCase) {
    let xs = tc.draw(gs::vecs(even_integers()));
    let total = xs.iter().fold(0i32, |acc, n| acc.wrapping_add(*n));
    assert_eq!(total % 2, 0);
}

mod module_alias {
    use hegel::prelude::*;

    #[hegel::test]
    fn test_gs_and_generators_name_the_same_module(tc: TestCase) {
        let g: generators::IntegerGenerator<i32> = gs::integers::<i32>();
        let n = tc.draw(g.min_value(0).max_value(10));
        assert!((0..=10).contains(&n));
    }
}

mod shadowing {
    use self::my_generators as gs;
    use hegel::prelude::*;

    mod my_generators {
        pub fn integers() -> &'static str {
            "shadowed"
        }
    }

    #[hegel::test]
    fn test_an_explicit_gs_shadows_the_prelude(tc: TestCase) {
        assert_eq!(gs::integers(), "shadowed");
        assert_eq!(tc.draw(super::even_integers()) % 2, 0);
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
