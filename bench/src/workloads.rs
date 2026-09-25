use hegel::generators::{self as gs, Generator};
use hegel::{DefaultGenerator, TestCase};

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

fn boolean(tc: &TestCase) {
    tc.draw(gs::booleans());
}

fn binary_64(tc: &TestCase) {
    tc.draw(gs::binary().max_size(64));
}

fn sampled_from_10(tc: &TestCase) {
    tc.draw(gs::sampled_from(vec![
        "a", "b", "c", "d", "e", "f", "g", "h", "i", "j",
    ]));
}

const SEMVER_GARBAGE: &str = "[0-9a-zA-Z.+*<>=^~ ,xX-]{0,20}";

fn regex_semver_garbage(tc: &TestCase) {
    tc.draw(gs::from_regex(SEMVER_GARBAGE));
}

fn one_of_text_regex(tc: &TestCase) {
    tc.draw(hegel::one_of!(
        gs::text().max_size(30),
        gs::from_regex(SEMVER_GARBAGE),
    ));
}

#[allow(dead_code)]
#[derive(Debug, Clone, DefaultGenerator)]
struct Version {
    major: u64,
    minor: u64,
    patch: u64,
    pre: String,
    build: Option<String>,
}

fn derived_struct(tc: &TestCase) {
    tc.draw(gs::default::<Version>());
}

fn draw_identifier(tc: &TestCase, max: usize) -> String {
    let n = tc.draw(gs::integers::<usize>().min_value(0).max_value(max));
    (0..n)
        .map(|_| {
            tc.draw(gs::sampled_from(vec![
                'a', 'b', 'c', 'x', 'y', 'z', '0', '1', '9', '-',
            ]))
        })
        .collect()
}

fn draw_version(tc: &TestCase) -> Version {
    let major = tc.draw(gs::integers::<u64>().max_value(1_000));
    let minor = tc.draw(gs::integers::<u64>().max_value(1_000));
    let patch = tc.draw(gs::integers::<u64>().max_value(1_000));
    let pre = if tc.draw(gs::booleans()) {
        draw_identifier(tc, 8)
    } else {
        String::new()
    };
    let build = tc
        .draw(gs::weighted_booleans(0.3))
        .then(|| draw_identifier(tc, 8));
    Version {
        major,
        minor,
        patch,
        pre,
        build,
    }
}

fn imperative_version(tc: &TestCase) {
    std::hint::black_box(draw_version(tc));
}

fn three_versions(tc: &TestCase) {
    std::hint::black_box((draw_version(tc), draw_version(tc), draw_version(tc)));
}

#[derive(Debug, Clone, hegel::PrettyPrintable)]
enum Tree {
    Leaf(i64),
    Branch(Vec<Tree>),
}

impl Tree {
    fn depth(&self) -> usize {
        match self {
            Tree::Leaf(_) => 0,
            Tree::Branch(children) => 1 + children.iter().map(Tree::depth).max().unwrap_or(0),
        }
    }
}

fn trees() -> impl hegel::PrintableGenerator<Tree> {
    gs::recursive(gs::integers::<i64>().map(Tree::Leaf), |sub| {
        gs::vecs(sub).max_size(4).map(Tree::Branch)
    })
}

fn recursive_tree(tc: &TestCase) {
    tc.draw(trees());
}

fn shrink_vec_bool_count(tc: &TestCase) {
    let v = tc.draw(gs::vecs(gs::booleans()).min_size(50).max_size(500));
    assert!(v.iter().filter(|&&b| b).count() < 20);
}

fn shrink_version(tc: &TestCase) {
    let v = draw_version(tc);
    assert!(!(v.major > 10 && !v.pre.is_empty()));
}

fn shrink_tree_depth(tc: &TestCase) {
    let t = tc.draw(trees());
    assert!(t.depth() < 3);
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
        generate("boolean", boolean),
        generate("binary_64", binary_64),
        generate("sampled_from_10", sampled_from_10),
        generate("regex_semver_garbage", regex_semver_garbage),
        generate("one_of_text_regex", one_of_text_regex),
        generate("derived_struct", derived_struct),
        generate("imperative_version", imperative_version),
        generate("three_versions", three_versions),
        generate("recursive_tree", recursive_tree),
        shrink("shrink_int_above_1000", shrink_int_above_1000),
        shrink("shrink_vec_sum", shrink_vec_sum),
        shrink("shrink_text_contains", shrink_text_contains),
        shrink("shrink_vec_bool_count", shrink_vec_bool_count),
        shrink("shrink_version", shrink_version),
        shrink("shrink_tree_depth", shrink_tree_depth),
    ]
}
