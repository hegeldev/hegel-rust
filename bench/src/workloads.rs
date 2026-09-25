use hegel::TestCase;
use hegel::generators::{self as gs, Generator};

use crate::{Kind, Workload};

fn int_i64(tc: &TestCase) {
    tc.draw(gs::integers::<i64>());
}

fn int_small_range(tc: &TestCase) {
    tc.draw(gs::integers::<u32>().min_value(0).max_value(100));
}

fn float_f64(tc: &TestCase) {
    tc.draw(gs::floats::<f64>());
}

fn vec_bool_1000(tc: &TestCase) {
    tc.draw(gs::vecs(gs::booleans()).max_size(1000));
}

fn vec_i32_100(tc: &TestCase) {
    tc.draw(gs::vecs(gs::integers::<i32>()).max_size(100));
}

fn text_default(tc: &TestCase) {
    tc.draw(gs::text());
}

fn text_ascii_64(tc: &TestCase) {
    tc.draw(
        gs::text()
            .alphabet("abcdefghijklmnopqrstuvwxyz0123456789 ")
            .max_size(64),
    );
}

fn vec_text_50(tc: &TestCase) {
    tc.draw(gs::vecs(gs::text().max_size(20)).max_size(50));
}

fn filtered_ints(tc: &TestCase) {
    tc.draw(gs::integers::<i64>().filter(|x| x % 3 == 0));
}

fn mapped_ints(tc: &TestCase) {
    tc.draw(gs::integers::<i32>().map(|x| x.to_string()));
}

fn shrink_int_above_1000(tc: &TestCase) {
    let n = tc.draw(gs::integers::<i64>());
    assert!(n <= 1000);
}

fn shrink_vec_sum(tc: &TestCase) {
    let v = tc.draw(gs::vecs(gs::integers::<i32>().min_value(0).max_value(1000)).max_size(100));
    let sum: i64 = v.iter().map(|&x| x as i64).sum();
    assert!(sum <= 1000);
}

fn shrink_text_contains(tc: &TestCase) {
    let s = tc.draw(gs::text().max_size(50));
    assert!(!(s.len() >= 3 && s.contains('a')));
}

pub fn all() -> Vec<Workload> {
    let generate = |name, body| Workload {
        name,
        kind: Kind::Generate,
        body,
    };
    let shrink = |name, body| Workload {
        name,
        kind: Kind::Shrink,
        body,
    };
    vec![
        generate("int_i64", int_i64),
        generate("int_small_range", int_small_range),
        generate("float_f64", float_f64),
        generate("vec_bool_1000", vec_bool_1000),
        generate("vec_i32_100", vec_i32_100),
        generate("text_default", text_default),
        generate("text_ascii_64", text_ascii_64),
        generate("vec_text_50", vec_text_50),
        generate("filtered_ints", filtered_ints),
        generate("mapped_ints", mapped_ints),
        shrink("shrink_int_above_1000", shrink_int_above_1000),
        shrink("shrink_vec_sum", shrink_vec_sum),
        shrink("shrink_text_contains", shrink_text_contains),
    ]
}
