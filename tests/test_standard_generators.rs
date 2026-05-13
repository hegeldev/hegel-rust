//! Tests verifying that standard generator types can produce examples.
//!
//! Each entry checks both the generator itself and a `vecs(generator)` wrapper,
//! mirroring Hypothesis's `test_draw_example.py / standard_types` parametrization.
//!
//! Python-only entries omitted:
//! - `complex_numbers()`, `fractions()`, `decimals()` — no Rust type
//! - `recursive(...)` — no `gs::recursive()`
//! - `sets(frozensets(booleans()))` — `HashSet` is not `Hash` in Rust
//!
//! Two macro forms.  `draw_example_tests!($name, $gen)` is the smoke
//! form: it asserts the generator runs to completion at all, but
//! makes no claim about the produced values beyond what the type
//! system already enforces.  `draw_example_tests_with_predicate!(
//! $name, $gen, $pred)` is the behavioural form: it asserts every
//! produced example satisfies `$pred`, and every element of
//! `vecs($gen)` does too.  Use the behavioural form whenever the
//! generator has an explicit bound or constraint that's *not*
//! implied by the type — e.g. `gs::integers().min_value(3)` should
//! produce only `>= 3` values, and the smoke form would be vapid.

mod common;

use std::collections::HashMap;

#[allow(dead_code)]
#[derive(Debug, Clone)]
enum IntOrBoolTuple {
    Int(i64),
    BoolTuple((bool,)),
}

macro_rules! draw_example_tests {
    ($name:ident, $gen:expr) => {
        #[allow(clippy::approx_constant, unused_imports)]
        mod $name {
            use super::*;
            use crate::common::utils::check_can_generate_examples;
            use hegel::generators::{self as gs, Generator as _};

            #[test]
            fn test_single_example() {
                check_can_generate_examples($gen);
            }

            #[test]
            fn test_list_example() {
                check_can_generate_examples(gs::vecs($gen));
            }
        }
    };
}

/// Behavioural form: assert every drawn value satisfies `$pred`.
/// `$pred` is re-evaluated as an expression at each use site, so
/// each invocation produces a fresh closure — the predicate
/// expression itself does not need to be `Copy`.
macro_rules! draw_example_tests_with_predicate {
    ($name:ident, $gen:expr, $pred:expr) => {
        #[allow(clippy::approx_constant, unused_imports)]
        mod $name {
            use super::*;
            use crate::common::utils::assert_all_examples;
            use hegel::generators::{self as gs, Generator as _};

            #[test]
            fn test_single_example() {
                assert_all_examples($gen, $pred);
            }

            #[test]
            fn test_list_example() {
                let pred = $pred;
                assert_all_examples(gs::vecs($gen), move |xs| xs.iter().all(&pred));
            }
        }
    };
}

draw_example_tests!(empty_list, gs::vecs(gs::unit()).max_size(0));
draw_example_tests!(empty_tuple, gs::tuples!());
draw_example_tests!(empty_set, gs::hashsets(gs::unit()).max_size(0));
draw_example_tests!(empty_fixed_dict, gs::just(HashMap::<i32, i32>::new()));
draw_example_tests!(
    abc_bools,
    gs::tuples!(gs::booleans(), gs::booleans(), gs::booleans())
);
draw_example_tests!(
    abc_bools_int,
    gs::tuples!(gs::booleans(), gs::booleans(), gs::integers::<i64>())
);
draw_example_tests!(
    fixed_dict_int_bool,
    gs::tuples!(gs::integers::<i64>(), gs::booleans())
);
draw_example_tests!(
    dict_bool_int,
    gs::hashmaps(gs::booleans(), gs::integers::<i64>())
);
draw_example_tests!(dict_text_bool, gs::hashmaps(gs::text(), gs::booleans()));
draw_example_tests!(
    one_of_int_or_bool_tuple,
    gs::one_of(vec![
        gs::integers::<i64>().map(IntOrBoolTuple::Int).boxed(),
        gs::tuples!(gs::booleans())
            .map(IntOrBoolTuple::BoolTuple)
            .boxed(),
    ])
);
draw_example_tests_with_predicate!(
    sampled_from_range,
    gs::sampled_from((0..10).collect::<Vec<i32>>()),
    |x: &i32| (0..10).contains(x)
);
draw_example_tests!(
    one_of_strings,
    gs::one_of(vec![
        gs::just("a".to_string()).boxed(),
        gs::just("b".to_string()).boxed(),
        gs::just("c".to_string()).boxed(),
    ])
);
draw_example_tests_with_predicate!(
    sampled_from_strings,
    gs::sampled_from(vec!["a", "b", "c"]),
    |x: &&str| matches!(*x, "a" | "b" | "c")
);
draw_example_tests!(integers, gs::integers::<i64>());
draw_example_tests_with_predicate!(
    integers_min,
    gs::integers::<i64>().min_value(3),
    |x: &i64| *x >= 3
);
draw_example_tests_with_predicate!(
    integers_wide_range,
    gs::integers::<i128>()
        .min_value(-(1i128 << 32))
        .max_value(1i128 << 64),
    |x: &i128| *x >= -(1i128 << 32) && *x <= 1i128 << 64
);
draw_example_tests!(floats, gs::floats::<f64>());
draw_example_tests_with_predicate!(
    floats_bounded,
    gs::floats::<f64>().min_value(-2.0).max_value(3.0),
    |x: &f64| (-2.0..=3.0).contains(x)
);
draw_example_tests_with_predicate!(
    floats_min_only,
    gs::floats::<f64>().min_value(-2.0),
    |x: &f64| *x >= -2.0
);
draw_example_tests_with_predicate!(
    floats_max_neg_zero,
    gs::floats::<f64>().max_value(-0.0),
    |x: &f64| *x <= -0.0
);
draw_example_tests_with_predicate!(
    floats_min_zero,
    gs::floats::<f64>().min_value(0.0),
    |x: &f64| *x >= 0.0
);
draw_example_tests_with_predicate!(
    floats_exact,
    gs::floats::<f64>().min_value(3.14).max_value(3.14),
    |x: &f64| *x == 3.14
);
draw_example_tests!(text, gs::text());
draw_example_tests!(binary, gs::binary());
draw_example_tests!(booleans, gs::booleans());
draw_example_tests!(tuple_booleans, gs::tuples!(gs::booleans(), gs::booleans()));
draw_example_tests!(hashsets_integers, gs::hashsets(gs::integers::<i64>()));
draw_example_tests!(nested_lists, gs::vecs(gs::vecs(gs::booleans())));
draw_example_tests_with_predicate!(
    list_exact_floats,
    gs::vecs(gs::floats::<f64>().min_value(0.0).max_value(0.0)),
    |xs: &Vec<f64>| xs.iter().all(|x| *x == 0.0)
);
draw_example_tests!(
    flatmap_ordered_pair,
    gs::integers::<i64>().flat_map(|right| {
        gs::integers::<i64>()
            .min_value(0)
            .map(move |length| (right.wrapping_sub(length), right))
    })
);
draw_example_tests!(
    flatmap_const_lists,
    gs::integers::<i64>().flat_map(|v| gs::vecs(gs::just(v)))
);
draw_example_tests_with_predicate!(
    filter_large_abs,
    gs::integers::<i64>().filter(|x: &i64| *x > 100 || *x < -100),
    |x: &i64| *x > 100 || *x < -100
);
draw_example_tests!(
    floats_full_range,
    gs::floats::<f64>().min_value(-f64::MAX).max_value(f64::MAX)
);
draw_example_tests!(unit, gs::unit());

#[cfg(feature = "rand")]
draw_example_tests!(randoms, hegel::extras::rand::randoms());

mod direct_strategies {
    //! The upstream file is a large parametrized suite that mixes portable
    //! generator-validation cases with many Python-specific ones. Ported here
    //! are the cases that use public API with a hegel-rust counterpart (invalid
    //! bounds → panic, valid arguments → generate, simple `@given` shapes).
    //!
    //! Not ported (Python-specific or no Rust counterpart):
    //! - `st.decimals`, `st.fractions`, `st.complex_numbers`, `st.slices`,
    //!   `st.iterables`, `st.nothing`, `st.shared`, `st.builds`, `st.data` —
    //!   no hegel-rust equivalents.
    //! - Parametrized cases passing wrong-typed kwargs (`min_value="fish"`,
    //!   `min_size=math.nan`, `alphabet=[1]`, `regex=123`, `elements="hi"`,
    //!   `unique_by=(...)`, `v="4"`) — Rust's type system rejects them at
    //!   compile time, so there is nothing to assert at runtime.
    //! - `st.ip_addresses(v=..., network=...)` — hegel-rust's
    //!   `IpAddressGenerator` only exposes `.v4()` / `.v6()`, no `network=`.
    //! - `st.dates/datetimes/times(min_value=..., max_value=...)` —
    //!   hegel-rust's date/time generators have no bounds methods.
    //! - `test_chained_filter_tracks_all_conditions` — inspects
    //!   `wrapped_strategy.flat_conditions`, an internal attribute.
    //! - `test_ipaddress_from_network_*`, `test_builds_*`, `test_tuples_raise_*`,
    //!   `test_data_explicitly_rejects_non_strategies`, shared-strategies tests —
    //!   all depend on Python-specific APIs above.

    use super::common::utils::{
        assert_all_examples, check_can_generate_examples, expect_panic, minimal,
    };
    use hegel::generators::{self as gs, Generator};
    use hegel::{Hegel, Settings};
    use std::collections::HashMap;

    // --- test_validates_keyword_arguments: invalid bounds panic at draw time ---

    fn expect_generator_panic<T, G>(generator: G, pattern: &str)
    where
        G: Generator<T> + 'static + std::panic::UnwindSafe,
        T: std::fmt::Debug + Send + 'static,
    {
        expect_panic(
            move || {
                Hegel::new(move |tc| {
                    tc.draw(&generator);
                })
                .settings(Settings::new().test_cases(1).database(None))
                .run();
            },
            pattern,
        );
    }

    #[test]
    fn test_integers_rejects_min_greater_than_max() {
        expect_generator_panic(
            gs::integers::<i64>().min_value(2).max_value(1),
            "max_value < min_value",
        );
    }

    #[test]
    fn test_lists_rejects_min_size_greater_than_max_size() {
        expect_generator_panic(
            gs::vecs(gs::integers::<i64>()).min_size(10).max_size(9),
            "max_size < min_size",
        );
    }

    #[test]
    fn test_text_rejects_min_size_greater_than_max_size() {
        expect_generator_panic(gs::text().min_size(10).max_size(9), "max_size < min_size");
    }

    #[test]
    fn test_binary_rejects_min_size_greater_than_max_size() {
        expect_generator_panic(gs::binary().min_size(10).max_size(9), "max_size < min_size");
    }

    #[test]
    fn test_hashmaps_rejects_min_size_greater_than_max_size() {
        expect_generator_panic(
            gs::hashmaps(gs::booleans(), gs::booleans())
                .min_size(10)
                .max_size(1),
            "max_size < min_size",
        );
    }

    #[test]
    fn test_hashsets_rejects_min_size_greater_than_max_size() {
        expect_generator_panic(
            gs::hashsets(gs::integers::<i64>()).min_size(10).max_size(1),
            "max_size < min_size",
        );
    }

    #[test]
    fn test_floats_rejects_min_greater_than_max() {
        expect_generator_panic(
            gs::floats::<f64>().max_value(0.0).min_value(1.0),
            "max_value < min_value",
        );
    }

    #[test]
    fn test_floats_rejects_plus_zero_minus_zero_range() {
        expect_generator_panic(
            gs::floats::<f64>().min_value(0.0).max_value(-0.0),
            "InvalidArgument",
        );
    }

    #[test]
    fn test_floats_rejects_allow_nan_with_min() {
        expect_generator_panic(
            gs::floats::<f64>().min_value(0.0).allow_nan(true),
            "allow_nan=true with min_value or max_value",
        );
    }

    #[test]
    fn test_floats_rejects_allow_nan_with_max() {
        expect_generator_panic(
            gs::floats::<f64>().max_value(0.0).allow_nan(true),
            "allow_nan=true with min_value or max_value",
        );
    }

    #[test]
    fn test_floats_rejects_allow_infinity_with_both_bounds() {
        expect_generator_panic(
            gs::floats::<f64>()
                .min_value(0.0)
                .max_value(1.0)
                .allow_infinity(true),
            "allow_infinity=true with both min_value and max_value",
        );
    }

    #[test]
    fn test_sampled_from_rejects_empty() {
        expect_panic(
            || {
                gs::sampled_from::<i64, _>(Vec::<i64>::new());
            },
            "sampled_from cannot be empty",
        );
    }

    #[test]
    fn test_text_alphabet_empty_with_min_size_panics() {
        // text(alphabet="", min_size=1): no characters to pick from and we require
        // at least one character — generation rejects this.
        expect_generator_panic(
            gs::text().alphabet("").min_size(1),
            "(?i)(alphabet|characters|empty|unsatisfiable|invalid|too.much)",
        );
    }

    // --- test_produces_valid_examples_from_keyword: valid kwargs generate fine ---

    #[test]
    fn test_integers_min_value() {
        check_can_generate_examples(gs::integers::<i64>().min_value(0));
        check_can_generate_examples(gs::integers::<i64>().min_value(11));
        check_can_generate_examples(gs::integers::<i64>().min_value(11).max_value(100));
        check_can_generate_examples(gs::integers::<i64>().max_value(0));
        check_can_generate_examples(gs::integers::<i64>().min_value(-2).max_value(-1));
        check_can_generate_examples(gs::integers::<i64>().min_value(12).max_value(12));
    }

    #[test]
    fn test_floats_valid_combinations() {
        check_can_generate_examples(gs::floats::<f64>());
        check_can_generate_examples(gs::floats::<f64>().min_value(1.0));
        check_can_generate_examples(gs::floats::<f64>().max_value(1.0));
        check_can_generate_examples(gs::floats::<f64>().min_value(f64::INFINITY));
        check_can_generate_examples(gs::floats::<f64>().max_value(f64::NEG_INFINITY));
        check_can_generate_examples(gs::floats::<f64>().min_value(-1.0).max_value(1.0));
        check_can_generate_examples(
            gs::floats::<f64>()
                .min_value(-1.0)
                .max_value(1.0)
                .allow_infinity(false),
        );
        check_can_generate_examples(gs::floats::<f64>().min_value(1.0).allow_nan(false));
        check_can_generate_examples(gs::floats::<f64>().max_value(1.0).allow_nan(false));
        check_can_generate_examples(
            gs::floats::<f64>()
                .min_value(-1.0)
                .max_value(1.0)
                .allow_nan(false),
        );
    }

    #[test]
    fn test_lists_valid_kwargs() {
        check_can_generate_examples(gs::vecs(gs::integers::<i64>()));
        check_can_generate_examples(gs::vecs(gs::integers::<i64>()).max_size(5));
        check_can_generate_examples(gs::vecs(gs::booleans()).min_size(5));
        check_can_generate_examples(gs::vecs(gs::booleans()).min_size(5).max_size(10));
    }

    #[test]
    fn test_sets_valid_kwargs() {
        check_can_generate_examples(
            gs::hashsets(gs::integers::<i64>())
                .min_size(10)
                .max_size(10),
        );
    }

    #[test]
    fn test_booleans_and_just_and_sampled_from() {
        check_can_generate_examples(gs::booleans());
        check_can_generate_examples(gs::just("hi"));
        check_can_generate_examples(gs::sampled_from(vec![1]));
        check_can_generate_examples(gs::sampled_from(vec![1, 2, 3]));
    }

    #[test]
    fn test_dictionaries_valid_kwargs() {
        check_can_generate_examples(gs::hashmaps(gs::booleans(), gs::integers::<i64>()));
    }

    #[test]
    fn test_text_alphabet_kwargs() {
        check_can_generate_examples(gs::text().alphabet("abc"));
        // Upstream also tests `alphabet=""` with default `min_size=0`, relying on
        // Hypothesis's rule that an empty alphabet yields only empty strings.
        // hegel-rust's server rejects an empty character set outright, so we only
        // exercise the non-empty-alphabet case here.
    }

    #[test]
    fn test_characters_codecs_and_categories() {
        check_can_generate_examples(gs::characters().codec("ascii"));
        check_can_generate_examples(gs::characters().codec("latin-1"));
        check_can_generate_examples(gs::characters().categories(&["N"]));
        check_can_generate_examples(gs::characters().exclude_categories(&[]));
        check_can_generate_examples(gs::characters().include_characters("a").codec("ascii"));
        check_can_generate_examples(gs::characters().exclude_characters("a"));
        check_can_generate_examples(gs::characters().categories(&["Nd"]));
        check_can_generate_examples(gs::characters().exclude_categories(&["Nd"]));
    }

    #[test]
    fn test_from_regex_alphabet_combinations() {
        check_can_generate_examples(
            gs::from_regex("abc").alphabet(gs::characters().codec("ascii")),
        );
    }

    #[test]
    fn test_ip_addresses_kwargs() {
        check_can_generate_examples(gs::ip_addresses());
        check_can_generate_examples(gs::ip_addresses().v4());
        check_can_generate_examples(gs::ip_addresses().v6());
    }

    // --- test_produces_valid_examples_from_args ---

    #[derive(Debug, Clone)]
    #[allow(dead_code)]
    enum BoolOrBoolTuple {
        Bool(bool),
        BoolTuple((bool,)),
    }

    #[test]
    fn test_one_of_mixed_types_generates() {
        // st.one_of(st.booleans(), st.tuples(st.booleans()))
        check_can_generate_examples(gs::one_of(vec![
            gs::booleans().map(BoolOrBoolTuple::Bool).boxed(),
            hegel::tuples!(gs::booleans())
                .map(BoolOrBoolTuple::BoolTuple)
                .boxed(),
        ]));
    }

    #[test]
    fn test_one_of_single_branch_generates() {
        check_can_generate_examples(gs::one_of(vec![gs::booleans().boxed()]));
    }

    #[test]
    fn test_text_and_binary_noarg_generate() {
        check_can_generate_examples(gs::text());
        check_can_generate_examples(gs::binary());
    }

    #[test]
    fn test_builds_equivalent_via_tuples_map() {
        // st.builds(lambda x, y: x + y, st.integers(), st.integers())
        check_can_generate_examples(
            hegel::tuples!(gs::integers::<i64>(), gs::integers::<i64>())
                .map(|(x, y): (i64, i64)| x.wrapping_add(y)),
        );
    }

    // --- direct @given-style tests ---

    #[test]
    fn test_has_specified_length() {
        assert_all_examples(
            gs::vecs(gs::booleans()).min_size(10).max_size(10),
            |xs: &Vec<bool>| xs.len() == 10,
        );
    }

    #[test]
    fn test_has_upper_bound() {
        assert_all_examples(gs::integers::<i64>().max_value(100), |x: &i64| *x <= 100);
    }

    #[test]
    fn test_has_lower_bound() {
        assert_all_examples(gs::integers::<i64>().min_value(100), |x: &i64| *x >= 100);
    }

    #[test]
    fn test_is_in_bounds() {
        assert_all_examples(
            gs::integers::<i64>().min_value(1).max_value(2),
            |x: &i64| (1..=2).contains(x),
        );
    }

    #[test]
    fn test_float_can_find_max_value_inf() {
        let v = minimal(gs::floats::<f64>().max_value(f64::INFINITY), |x: &f64| {
            x.is_infinite()
        });
        assert_eq!(v, f64::INFINITY);
        let v = minimal(gs::floats::<f64>().min_value(0.0), |x: &f64| {
            x.is_infinite()
        });
        assert_eq!(v, f64::INFINITY);
    }

    #[test]
    fn test_float_can_find_min_value_inf() {
        // Unbounded: finds some negative infinity.
        let v = minimal(gs::floats::<f64>(), |x: &f64| *x < 0.0 && x.is_infinite());
        assert!(v.is_infinite() && v < 0.0);
        // The second upstream assertion (min_value=-inf, max_value=0.0) is not
        // portable: hegel-rust defaults `allow_infinity` to false when both
        // bounds are set, and overriding panics when both bounds are set.
    }

    #[test]
    fn test_can_find_none_list() {
        // Python `st.lists(st.none())` with `len(x) >= 3`: minimal is [None]*3.
        let v = minimal(gs::vecs(gs::unit()), |x: &Vec<()>| x.len() >= 3);
        assert_eq!(v, vec![(), (), ()]);
    }

    #[test]
    fn test_produces_dictionaries_of_at_least_minimum_size() {
        // Python test draws until it finds a 2-entry {False:0, True:0}.
        // Rust equivalent: any dict with min_size=2 and bool keys must contain
        // both False and True (the only two keys), and the minimal integer is 0.
        let v = minimal(
            gs::hashmaps(gs::booleans(), gs::integers::<i64>()).min_size(2),
            |_| true,
        );
        let mut expected: HashMap<bool, i64> = HashMap::new();
        expected.insert(false, 0);
        expected.insert(true, 0);
        assert_eq!(v, expected);
    }

    #[test]
    fn test_dictionaries_respect_size() {
        assert_all_examples(
            gs::hashmaps(gs::integers::<i64>(), gs::integers::<i64>()).max_size(5),
            |d: &HashMap<i64, i64>| d.len() <= 5,
        );
    }

    #[test]
    fn test_dictionaries_respect_zero_size() {
        assert_all_examples(
            gs::hashmaps(gs::integers::<i64>(), gs::integers::<i64>()).max_size(0),
            |d: &HashMap<i64, i64>| d.is_empty(),
        );
    }

    #[test]
    fn test_none_lists_respect_max_size() {
        assert_all_examples(gs::vecs(gs::unit()).max_size(5), |xs: &Vec<()>| {
            xs.len() <= 5
        });
    }

    #[test]
    fn test_none_lists_respect_max_and_min_size() {
        assert_all_examples(
            gs::vecs(gs::unit()).min_size(1).max_size(5),
            |xs: &Vec<()>| (1..=5).contains(&xs.len()),
        );
    }

    #[test]
    fn test_no_infinity_for_min_value_values() {
        for value in [-1.0_f64, 0.0, 1.0] {
            assert_all_examples(
                gs::floats::<f64>().allow_infinity(false).min_value(value),
                |x: &f64| !x.is_infinite(),
            );
        }
    }

    #[test]
    fn test_no_infinity_for_max_value_values() {
        for value in [-1.0_f64, 0.0, 1.0] {
            assert_all_examples(
                gs::floats::<f64>().allow_infinity(false).max_value(value),
                |x: &f64| !x.is_infinite(),
            );
        }
    }

    #[test]
    fn test_no_nan_for_min_value_values() {
        for value in [-1.0_f64, 0.0, 1.0] {
            assert_all_examples(
                gs::floats::<f64>().allow_nan(false).min_value(value),
                |x: &f64| !x.is_nan(),
            );
        }
    }

    #[test]
    fn test_no_nan_for_max_value_values() {
        for value in [-1.0_f64, 0.0, 1.0] {
            assert_all_examples(
                gs::floats::<f64>().allow_nan(false).max_value(value),
                |x: &f64| !x.is_nan(),
            );
        }
    }

    #[test]
    fn test_chained_filter() {
        // Python `st.integers().filter(bool).filter(lambda x: x % 3)`:
        // `bool(x)` is `x != 0`; `x % 3` truthy is `x % 3 != 0`.
        assert_all_examples(
            gs::integers::<i64>()
                .filter(|x: &i64| *x != 0)
                .filter(|x: &i64| x % 3 != 0),
            |x: &i64| *x != 0 && x % 3 != 0,
        );
    }
}

mod provisional_strategies {
    use std::collections::HashSet;

    use regex::Regex;

    use super::common::utils::{
        assert_all_examples, check_can_generate_examples, expect_panic, find_any,
    };
    use hegel::generators::{self as gs, Generator};

    fn url_allowed_chars() -> HashSet<char> {
        ('a'..='z')
            .chain('A'..='Z')
            .chain('0'..='9')
            .chain("$-_.+!*'(),~%/".chars())
            .collect()
    }

    #[test]
    fn test_is_url() {
        let allowed = url_allowed_chars();
        let mut fragment_allowed = allowed.clone();
        fragment_allowed.insert('?');
        let hex_pair = Regex::new(r"^[0-9A-Fa-f]{2}").unwrap();

        assert_all_examples(gs::urls(), move |url: &String| {
            let url_schemeless = match url.split_once("://") {
                Some((_, rest)) => rest,
                None => return false,
            };
            let (domain_path, fragment) = match url_schemeless.split_once('#') {
                Some((dp, fr)) => (dp, fr),
                None => (url_schemeless, ""),
            };
            let path = domain_path.split_once('/').map_or("", |(_, p)| p);

            if !path.chars().all(|c| allowed.contains(&c)) {
                return false;
            }
            for after_perc in path.split('%').skip(1) {
                if !hex_pair.is_match(after_perc) {
                    return false;
                }
            }

            if !fragment.chars().all(|c| fragment_allowed.contains(&c)) {
                return false;
            }
            for after_perc in fragment.split('%').skip(1) {
                if !hex_pair.is_match(after_perc) {
                    return false;
                }
            }
            true
        });
    }

    #[test]
    fn test_invalid_domain_arguments() {
        for max_length in [0_usize, 3, 256] {
            expect_panic(
                move || {
                    gs::domains().max_length(max_length).as_basic();
                },
                "max_length must be between 4 and 255",
            );
        }
    }

    #[test]
    fn test_valid_domains_arguments() {
        check_can_generate_examples(gs::domains());
        for max_length in [4_usize, 8, 255] {
            check_can_generate_examples(gs::domains().max_length(max_length));
        }
    }

    #[test]
    fn test_find_any_non_empty_domains() {
        find_any(gs::domains(), |s: &String| !s.is_empty());
    }

    #[test]
    fn test_find_any_non_empty_urls() {
        find_any(gs::urls(), |s: &String| !s.is_empty());
    }
}

mod pbtkit_generators {
    use std::collections::HashMap;

    use super::common::utils::{assert_all_examples, expect_panic, minimal};
    use hegel::generators::{self as gs, Generator};
    use hegel::{Hegel, Settings};

    #[test]
    fn test_mapped_possibility() {
        assert_all_examples(
            gs::integers::<i64>()
                .min_value(0)
                .max_value(5)
                .map(|n| n * 2),
            |n: &i64| n % 2 == 0,
        );
    }

    #[test]
    fn test_selected_possibility() {
        assert_all_examples(
            gs::integers::<i64>()
                .min_value(0)
                .max_value(5)
                .filter(|n: &i64| n % 2 == 0),
            |n: &i64| n % 2 == 0,
        );
    }

    #[test]
    fn test_bound_possibility() {
        assert_all_examples(
            gs::integers::<i64>()
                .min_value(0)
                .max_value(5)
                .flat_map(|m| {
                    gs::tuples!(
                        gs::just(m),
                        gs::integers::<i64>().min_value(m).max_value(m + 10),
                    )
                }),
            |(m, n): &(i64, i64)| *m <= *n && *n <= *m + 10,
        );
    }

    #[test]
    fn test_cannot_witness_empty_one_of() {
        // Python raises Unsatisfiable when drawing from one_of() with no
        // alternatives; hegel-rust panics at construction instead.
        expect_panic(
            || {
                let empty: Vec<gs::BoxedGenerator<i32>> = vec![];
                gs::one_of(empty);
            },
            "one_of requires at least one generator",
        );
    }

    #[test]
    fn test_one_of_single() {
        assert_all_examples(
            hegel::one_of!(gs::integers::<i64>().min_value(0).max_value(10)),
            |n: &i64| (0..=10).contains(n),
        );
    }

    #[test]
    fn test_can_draw_mixture() {
        assert_all_examples(
            hegel::one_of!(
                gs::integers::<i64>().min_value(-5).max_value(0),
                gs::integers::<i64>().min_value(2).max_value(5),
            ),
            |m: &i64| (-5..=5).contains(m) && *m != 1,
        );
    }

    #[test]
    fn test_size_bounds_on_list() {
        assert_all_examples(
            gs::vecs(gs::integers::<i64>().min_value(0).max_value(10))
                .min_size(1)
                .max_size(3),
            |ls: &Vec<i64>| (1..=3).contains(&ls.len()),
        );
    }

    #[test]
    fn test_fixed_size_list() {
        assert_all_examples(
            gs::vecs(gs::integers::<i64>().min_value(0).max_value(10))
                .min_size(3)
                .max_size(3),
            |ls: &Vec<i64>| ls.len() == 3,
        );
    }

    #[test]
    fn test_many_with_small_max() {
        // Exercises the geometric-distribution path for collections with a
        // small max_size.
        Hegel::new(|tc| {
            let ls: Vec<i64> =
                tc.draw(gs::vecs(gs::integers::<i64>().min_value(0).max_value(10)).max_size(2));
            assert!(ls.len() <= 2);
        })
        .settings(Settings::new().test_cases(200).database(None))
        .run();
    }

    #[test]
    fn test_sampled_from() {
        assert_all_examples(gs::sampled_from(vec!["a", "b", "c"]), |v: &&'static str| {
            matches!(*v, "a" | "b" | "c")
        });
    }

    #[test]
    fn test_sampled_from_shrinks_to_first() {
        // Python test asserts "'a'" appears in the failure output. Using
        // `minimal`, the minimum generated value that triggers the condition
        // should be the first element of the sample list.
        let v = minimal(
            gs::sampled_from(vec!["a".to_string(), "b".to_string(), "c".to_string()]),
            |v: &String| v == "a",
        );
        assert_eq!(v, "a");
    }

    #[test]
    fn test_sampled_from_single() {
        assert_all_examples(gs::sampled_from(vec!["only"]), |v: &&'static str| {
            *v == "only"
        });
    }

    #[test]
    fn test_sampled_from_empty() {
        expect_panic(
            || {
                let empty: Vec<i32> = vec![];
                gs::sampled_from(empty);
            },
            "cannot be empty",
        );
    }

    #[test]
    fn test_booleans() {
        assert_all_examples(gs::booleans(), |_: &bool| true);
    }

    #[test]
    fn test_composite() {
        assert_all_examples(
            hegel::compose!(|tc| {
                let x: i64 = tc.draw(gs::integers::<i64>().min_value(0).max_value(10));
                let y: i64 = tc.draw(gs::integers::<i64>().min_value(x).max_value(10));
                (x, y)
            }),
            |(x, y): &(i64, i64)| *x <= *y && *y <= 10,
        );
    }

    #[test]
    fn test_composite_with_args() {
        let max_val: i64 = 5;
        assert_all_examples(
            hegel::compose!(|tc| {
                tc.draw(gs::integers::<i64>().min_value(0).max_value(max_val))
            }),
            |n: &i64| (0..=5).contains(n),
        );
    }

    #[test]
    fn test_composite_shrinks() {
        // Python test asserts the shrunk counterexample is "100, 0" or "0, 100".
        // We check the same property: shrinking lands exactly on the boundary.
        let (x, y) = minimal(
            hegel::compose!(|tc| {
                let x: i64 = tc.draw(gs::integers::<i64>().min_value(0).max_value(100));
                let y: i64 = tc.draw(gs::integers::<i64>().min_value(0).max_value(100));
                (x, y)
            }),
            |(x, y): &(i64, i64)| x + y >= 100,
        );
        assert_eq!(x + y, 100);
        assert!((x == 100 && y == 0) || (x == 0 && y == 100));
    }

    #[test]
    fn test_unique_lists() {
        assert_all_examples(
            gs::vecs(gs::integers::<i64>().min_value(0).max_value(10))
                .unique(true)
                .max_size(5),
            |ls: &Vec<i64>| {
                let mut seen = std::collections::HashSet::new();
                ls.iter().all(|x| seen.insert(*x))
            },
        );
    }

    #[test]
    fn test_unique_lists_shrinks() {
        let ls = minimal(
            gs::vecs(gs::integers::<i64>().min_value(0).max_value(100)).unique(true),
            |ls: &Vec<i64>| ls.len() >= 3,
        );
        assert_eq!(ls.len(), 3);
    }

    #[test]
    fn test_dictionaries() {
        assert_all_examples(
            gs::hashmaps(
                gs::integers::<i64>().min_value(0).max_value(10),
                gs::integers::<i64>().min_value(0).max_value(100),
            )
            .max_size(5),
            |d: &HashMap<i64, i64>| {
                d.len() <= 5
                    && d.iter()
                        .all(|(k, v)| (0..=10).contains(k) && (0..=100).contains(v))
            },
        );
    }

    #[test]
    fn test_dictionaries_shrinks() {
        let d = minimal(
            gs::hashmaps(
                gs::integers::<i64>().min_value(0).max_value(10),
                gs::integers::<i64>().min_value(0).max_value(100),
            ),
            |d: &HashMap<i64, i64>| d.values().sum::<i64>() > 100,
        );
        assert!(d.values().sum::<i64>() > 100);
    }

    #[test]
    fn test_dictionaries_size_bounds() {
        assert_all_examples(
            gs::hashmaps(
                gs::integers::<i64>().min_value(0).max_value(10),
                gs::integers::<i64>().min_value(0).max_value(100),
            )
            .min_size(1)
            .max_size(3),
            |d: &HashMap<i64, i64>| (1..=3).contains(&d.len()),
        );
    }
}
