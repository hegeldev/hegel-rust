//! Ported from resources/hypothesis/hypothesis-python/tests/cover/test_direct_strategies.py
//!
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

use crate::common::utils::{
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
    // at least one character — native/server generation rejects this.
    expect_generator_panic(
        gs::text().alphabet("").min_size(1),
        "(?i)(alphabet|characters|empty|unsatisfiable|invalid|too.much)",
    );
}

#[test]
fn test_uuids_version_and_allow_nil_panics() {
    expect_generator_panic(
        gs::uuids().version(4).allow_nil(true),
        "nil UUID is not of any version",
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
    check_can_generate_examples(gs::from_regex("abc").alphabet(gs::characters().codec("ascii")));
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
