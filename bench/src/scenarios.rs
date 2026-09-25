use hegel::generators::{self as gs, Generator};
use hegel::{DefaultGenerator, TestCase};

use crate::{Config, Kind, Scenario, generate, shrink};

macro_rules! scenarios {
    ($($kind:ident $name:ident;)*) => {
        pub fn all() -> Vec<&'static Scenario> {
            vec![$(&Scenario { name: stringify!($name), kind: Kind::$kind, run: $name }),*]
        }
    };
}

scenarios! {
    Generate empty;
    Generate booleans;
    Generate integers_i64;
    Generate integers_0_100;
    Generate floats;
    Generate text_30;
    Generate binary_64;
    Generate sampled_from_10;
    Generate vec_i32_50;
    Generate vec_bool_1000;
    Generate map_filter;
    Generate from_regex_semver_garbage;
    Generate one_of_text_regex;
    Generate derive_struct;
    Generate imperative_version;
    Generate recursive_tree;
    Generate zoo_semver_ord_axioms;
    Shrink shrink_vec_sum;
    Shrink shrink_vec_bool_count;
    Shrink shrink_text_alpha;
    Shrink shrink_version;
    Shrink shrink_tree_depth;
}

fn empty(cfg: &Config) {
    generate(cfg, |_tc| {});
}

fn booleans(cfg: &Config) {
    generate(cfg, |tc| {
        tc.draw(gs::booleans());
    });
}

fn integers_i64(cfg: &Config) {
    generate(cfg, |tc| {
        tc.draw(gs::integers::<i64>());
    });
}

fn integers_0_100(cfg: &Config) {
    generate(cfg, |tc| {
        tc.draw(gs::integers::<i32>().min_value(0).max_value(100));
    });
}

fn floats(cfg: &Config) {
    generate(cfg, |tc| {
        tc.draw(gs::floats::<f64>());
    });
}

fn text_30(cfg: &Config) {
    generate(cfg, |tc| {
        tc.draw(gs::text().max_size(30));
    });
}

fn binary_64(cfg: &Config) {
    generate(cfg, |tc| {
        tc.draw(gs::binary().max_size(64));
    });
}

fn sampled_from_10(cfg: &Config) {
    let items: Vec<&'static str> = vec!["a", "b", "c", "d", "e", "f", "g", "h", "i", "j"];
    generate(cfg, |tc| {
        tc.draw(gs::sampled_from(items.clone()));
    });
}

fn vec_i32_50(cfg: &Config) {
    generate(cfg, |tc| {
        tc.draw(gs::vecs(gs::integers::<i32>()).max_size(50));
    });
}

fn vec_bool_1000(cfg: &Config) {
    generate(cfg, |tc| {
        tc.draw(gs::vecs(gs::booleans()).min_size(1000).max_size(1000));
    });
}

fn map_filter(cfg: &Config) {
    generate(cfg, |tc| {
        tc.draw(
            gs::integers::<i64>()
                .map(|x| x.wrapping_mul(2))
                .filter(|x| x % 3 == 0),
        );
    });
}

const SEMVER_GARBAGE: &str = "[0-9a-zA-Z.+*<>=^~ ,xX-]{0,20}";

fn from_regex_semver_garbage(cfg: &Config) {
    generate(cfg, |tc| {
        tc.draw(gs::from_regex(SEMVER_GARBAGE));
    });
}

fn one_of_text_regex(cfg: &Config) {
    generate(cfg, |tc| {
        tc.draw(hegel::one_of!(
            gs::text().max_size(30),
            gs::from_regex(SEMVER_GARBAGE),
        ));
    });
}

#[derive(Debug, Clone, DefaultGenerator)]
struct Version {
    major: u64,
    minor: u64,
    patch: u64,
    pre: String,
    build: Option<String>,
}

fn derive_struct(cfg: &Config) {
    generate(cfg, |tc| {
        tc.draw(gs::default::<Version>());
    });
}

fn draw_identifier(tc: &TestCase, max: usize) -> String {
    let n = tc.draw(gs::integers::<usize>().min_value(0).max_value(max));
    let mut s = String::new();
    for _ in 0..n {
        let c = tc.draw(gs::sampled_from(vec![
            'a', 'b', 'c', 'x', 'y', 'z', '0', '1', '9', '-',
        ]));
        s.push(c);
    }
    s
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
    let build = if tc.draw(gs::weighted_booleans(0.3)) {
        Some(draw_identifier(tc, 8))
    } else {
        None
    };
    Version {
        major,
        minor,
        patch,
        pre,
        build,
    }
}

fn imperative_version(cfg: &Config) {
    generate(cfg, |tc| {
        std::hint::black_box(draw_version(&tc));
    });
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

fn recursive_tree(cfg: &Config) {
    generate(cfg, |tc| {
        tc.draw(trees());
    });
}

fn zoo_semver_ord_axioms(cfg: &Config) {
    generate(cfg, |tc| {
        let a = draw_version(&tc);
        let b = draw_version(&tc);
        let c = draw_version(&tc);
        std::hint::black_box((a, b, c));
    });
}

fn shrink_vec_sum(cfg: &Config) {
    shrink(cfg, |tc| {
        let v = tc.draw(gs::vecs(gs::integers::<i32>()).max_size(100));
        let sum: i64 = v.iter().map(|&x| x as i64).sum();
        assert!(sum <= 1_000_000, "sum too large");
    });
}

fn shrink_vec_bool_count(cfg: &Config) {
    shrink(cfg, |tc| {
        let v = tc.draw(gs::vecs(gs::booleans()).min_size(50).max_size(500));
        assert!(v.iter().filter(|&&b| b).count() < 20, "too many trues");
    });
}

fn shrink_text_alpha(cfg: &Config) {
    shrink(cfg, |tc| {
        let s = tc.draw(gs::text().max_size(50));
        assert!(
            s.chars().filter(|c| c.is_alphabetic()).count() < 5,
            "too many letters"
        );
    });
}

fn shrink_version(cfg: &Config) {
    shrink(cfg, |tc| {
        let v = draw_version(&tc);
        assert!(!(v.major > 10 && !v.pre.is_empty()), "bad version");
    });
}

fn shrink_tree_depth(cfg: &Config) {
    shrink(cfg, |tc| {
        let t = tc.draw(trees());
        assert!(t.depth() < 3, "too deep");
    });
}
