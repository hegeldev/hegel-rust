//! Ported from hypothesis-python/tests/cover/test_draw_example.py
//!
//! The original parametrizes over `standard_types` and calls
//! `check_can_generate_examples` on each (single and wrapped in `lists()`).
//!
//! Python-only entries omitted from `standard_types`:
//! - `complex_numbers()`, `fractions()`, `decimals()` — no Rust type
//! - `recursive(base=booleans(), extend=..., max_leaves=10)` — no `gs::recursive()`
//! - `booleans().flatmap(lambda x: booleans() if x else complex_numbers())` — uses complex_numbers
//! - `sets(frozensets(booleans()))` — `HashSet` is not `Hash` in Rust

use std::collections::HashMap;

use crate::common::utils::check_can_generate_examples;
use hegel::generators::{self as gs, Generator};

#[derive(Debug, Clone)]
enum IntOrBoolTuple {
    Int(i64),
    BoolTuple((bool,)),
}

macro_rules! draw_example_tests {
    ($name:ident, $gen:expr) => {
        mod $name {
            use super::*;
            use crate::common::utils::check_can_generate_examples;
            use hegel::generators::{self as gs, Generator};

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

// lists(none(), max_size=0)
draw_example_tests!(empty_list, gs::vecs(gs::unit()).max_size(0));

// tuples()
draw_example_tests!(empty_tuple, gs::tuples!());

// sets(none(), max_size=0)
draw_example_tests!(empty_set, gs::hashsets(gs::unit()).max_size(0));

// frozensets(none(), max_size=0)
draw_example_tests!(empty_frozenset, gs::hashsets(gs::unit()).max_size(0));

// fixed_dictionaries({})
draw_example_tests!(empty_fixed_dict, gs::just(HashMap::<i32, i32>::new()));

// abc(booleans(), booleans(), booleans())
draw_example_tests!(
    abc_bools,
    gs::tuples!(gs::booleans(), gs::booleans(), gs::booleans())
);

// abc(booleans(), booleans(), integers())
draw_example_tests!(
    abc_bools_int,
    gs::tuples!(gs::booleans(), gs::booleans(), gs::integers::<i64>())
);

// fixed_dictionaries({"a": integers(), "b": booleans()})
draw_example_tests!(
    fixed_dict_int_bool,
    gs::tuples!(gs::integers::<i64>(), gs::booleans())
);

// dictionaries(booleans(), integers())
draw_example_tests!(
    dict_bool_int,
    gs::hashmaps(gs::booleans(), gs::integers::<i64>())
);

// dictionaries(text(), booleans())
draw_example_tests!(dict_text_bool, gs::hashmaps(gs::text(), gs::booleans()));

// one_of(integers(), tuples(booleans()))
draw_example_tests!(
    one_of_int_or_bool_tuple,
    gs::one_of(vec![
        gs::integers::<i64>().map(IntOrBoolTuple::Int).boxed(),
        gs::tuples!(gs::booleans())
            .map(IntOrBoolTuple::BoolTuple)
            .boxed(),
    ])
);

// sampled_from(range(10))
draw_example_tests!(
    sampled_from_range,
    gs::sampled_from((0..10).collect::<Vec<i32>>())
);

// one_of(just("a"), just("b"), just("c"))
draw_example_tests!(
    one_of_strings,
    gs::one_of(vec![
        gs::just("a".to_string()).boxed(),
        gs::just("b".to_string()).boxed(),
        gs::just("c".to_string()).boxed(),
    ])
);

// sampled_from(("a", "b", "c"))
draw_example_tests!(
    sampled_from_strings,
    gs::sampled_from(vec!["a", "b", "c"])
);

// integers()
draw_example_tests!(integers, gs::integers::<i64>());

// integers(min_value=3)
draw_example_tests!(integers_min, gs::integers::<i64>().min_value(3));

// integers(min_value=-(2**32), max_value=2**64)
draw_example_tests!(
    integers_wide_range,
    gs::integers::<i128>()
        .min_value(-(1i128 << 32))
        .max_value(1i128 << 64)
);

// floats()
draw_example_tests!(floats, gs::floats::<f64>());

// floats(min_value=-2.0, max_value=3.0)
draw_example_tests!(
    floats_bounded,
    gs::floats::<f64>().min_value(-2.0).max_value(3.0)
);

// floats(min_value=-2.0)
draw_example_tests!(floats_min_only, gs::floats::<f64>().min_value(-2.0));

// floats(max_value=-0.0)
draw_example_tests!(floats_max_neg_zero, gs::floats::<f64>().max_value(-0.0));

// floats(min_value=0.0)
draw_example_tests!(floats_min_zero, gs::floats::<f64>().min_value(0.0));

// floats(min_value=3.14, max_value=3.14)
draw_example_tests!(
    floats_exact,
    gs::floats::<f64>().min_value(3.14).max_value(3.14)
);

// text()
draw_example_tests!(text, gs::text());

// binary()
draw_example_tests!(binary, gs::binary());

// booleans()
draw_example_tests!(booleans, gs::booleans());

// tuples(booleans(), booleans())
draw_example_tests!(
    tuple_booleans,
    gs::tuples!(gs::booleans(), gs::booleans())
);

// frozensets(integers())
draw_example_tests!(hashsets_integers, gs::hashsets(gs::integers::<i64>()));

// lists(lists(booleans()))
draw_example_tests!(nested_lists, gs::vecs(gs::vecs(gs::booleans())));

// lists(floats(0.0, 0.0))
draw_example_tests!(
    list_exact_floats,
    gs::vecs(gs::floats::<f64>().min_value(0.0).max_value(0.0))
);

// integers().flatmap(lambda right: integers(min_value=0).map(lambda length: OrderedPair(right - length, right)))
draw_example_tests!(
    flatmap_ordered_pair,
    gs::integers::<i64>().flat_map(|right| {
        gs::integers::<i64>()
            .min_value(0)
            .map(move |length| (right.wrapping_sub(length), right))
    })
);

// integers().flatmap(lambda v: lists(just(v)))
draw_example_tests!(
    flatmap_const_lists,
    gs::integers::<i64>().flat_map(|v| gs::vecs(gs::just(v)))
);

// integers().filter(lambda x: abs(x) > 100)
draw_example_tests!(
    filter_large_abs,
    gs::integers::<i64>().filter(|x: &i64| *x > 100 || *x < -100)
);

// floats(min_value=-sys.float_info.max, max_value=sys.float_info.max)
draw_example_tests!(
    floats_full_range,
    gs::floats::<f64>().min_value(-f64::MAX).max_value(f64::MAX)
);

// none()
draw_example_tests!(unit, gs::unit());

// randoms(use_true_random=True)
#[cfg(feature = "rand")]
draw_example_tests!(randoms, gs::randoms());
