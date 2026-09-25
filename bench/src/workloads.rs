use hegel::generators::{self as gs, Generator};
use hegel::stateful::{Pool, machine, pool};
use hegel::{DefaultGenerator, TestCase};

use crate::{Kind, Workload};

fn int_i64(tc: TestCase) {
    tc.draw(gs::integers::<i64>());
}

fn int_small_range(tc: TestCase) {
    tc.draw(gs::integers::<u32>().min_value(0).max_value(100));
}

fn float_f64(tc: TestCase) {
    tc.draw(gs::floats::<f64>());
}

fn vec_bool_1000(tc: TestCase) {
    tc.draw(gs::vecs(gs::booleans()).max_size(1000));
}

fn vec_i32_100(tc: TestCase) {
    tc.draw(gs::vecs(gs::integers::<i32>()).max_size(100));
}

fn text_default(tc: TestCase) {
    tc.draw(gs::text());
}

fn text_ascii_64(tc: TestCase) {
    tc.draw(
        gs::text()
            .alphabet("abcdefghijklmnopqrstuvwxyz0123456789 ")
            .max_size(64),
    );
}

fn vec_text_50(tc: TestCase) {
    tc.draw(gs::vecs(gs::text().max_size(20)).max_size(50));
}

fn filtered_ints(tc: TestCase) {
    tc.draw(gs::integers::<i64>().filter(|x| x % 3 == 0));
}

fn mapped_ints(tc: TestCase) {
    tc.draw(gs::integers::<i32>().map(|x| x.to_string()));
}

fn shrink_int_above_1000(tc: TestCase) {
    let n = tc.draw(gs::integers::<i64>());
    assert!(n <= 1000);
}

fn shrink_vec_sum(tc: TestCase) {
    let v = tc.draw(gs::vecs(gs::integers::<i32>().min_value(0).max_value(1000)).max_size(100));
    let sum: i64 = v.iter().map(|&x| x as i64).sum();
    assert!(sum <= 1000);
}

fn shrink_text_contains(tc: TestCase) {
    let s = tc.draw(gs::text().max_size(50));
    assert!(!(s.len() >= 3 && s.contains('a')));
}

fn boolean(tc: TestCase) {
    tc.draw(gs::booleans());
}

fn binary_64(tc: TestCase) {
    tc.draw(gs::binary().max_size(64));
}

fn sampled_from_10(tc: TestCase) {
    tc.draw(gs::sampled_from(vec![
        "a", "b", "c", "d", "e", "f", "g", "h", "i", "j",
    ]));
}

const SEMVER_GARBAGE: &str = "[0-9a-zA-Z.+*<>=^~ ,xX-]{0,20}";

fn regex_semver_garbage(tc: TestCase) {
    tc.draw(gs::from_regex(SEMVER_GARBAGE));
}

fn one_of_text_regex(tc: TestCase) {
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

fn derived_struct(tc: TestCase) {
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

fn imperative_version(tc: TestCase) {
    std::hint::black_box(draw_version(&tc));
}

fn three_versions(tc: TestCase) {
    std::hint::black_box((draw_version(&tc), draw_version(&tc), draw_version(&tc)));
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

fn recursive_tree(tc: TestCase) {
    tc.draw(trees());
}

fn shrink_vec_bool_count(tc: TestCase) {
    let v = tc.draw(gs::vecs(gs::booleans()).min_size(50).max_size(500));
    assert!(v.iter().filter(|&&b| b).count() < 20);
}

fn shrink_version(tc: TestCase) {
    let v = draw_version(&tc);
    assert!(!(v.major > 10 && !v.pre.is_empty()));
}

fn shrink_tree_depth(tc: TestCase) {
    let t = tc.draw(trees());
    assert!(t.depth() < 3);
}

fn hashmap_i32_50(tc: TestCase) {
    tc.draw(gs::hashmaps(gs::integers::<i32>(), gs::integers::<i32>()).max_size(50));
}

fn regex_word_digits(tc: TestCase) {
    tc.draw(gs::from_regex(r"\w{1,8}-\d{1,4}"));
}

fn draw_map_key(tc: &TestCase) -> u16 {
    tc.draw(hegel::one_of!(
        gs::integers::<u16>().max_value(7),
        gs::integers::<u16>(),
    ))
}

struct MapMachine {
    map: std::collections::HashMap<u16, i32>,
    model: Vec<(u16, i32)>,
    limit: Option<usize>,
}

impl MapMachine {
    fn new(limit: Option<usize>) -> Self {
        MapMachine {
            map: std::collections::HashMap::new(),
            model: Vec::new(),
            limit,
        }
    }

    fn position(&self, k: u16) -> Option<usize> {
        self.model.iter().position(|&(mk, _)| mk == k)
    }
}

#[hegel::state_machine]
impl MapMachine {
    #[rule]
    fn insert(&mut self, tc: TestCase) {
        let k = draw_map_key(&tc);
        let v = tc.draw(gs::integers::<i32>());
        let old = self.map.insert(k, v);
        let model_old = match self.position(k) {
            Some(i) => Some(std::mem::replace(&mut self.model[i].1, v)),
            None => {
                self.model.push((k, v));
                None
            }
        };
        assert_eq!(old, model_old);
        if let Some(limit) = self.limit {
            assert!(self.map.len() < limit);
        }
    }

    #[rule]
    fn remove(&mut self, tc: TestCase) {
        let k = draw_map_key(&tc);
        let removed = self.map.remove(&k);
        let model_removed = self.position(k).map(|i| self.model.swap_remove(i).1);
        assert_eq!(removed, model_removed);
    }

    #[rule]
    fn get(&mut self, tc: TestCase) {
        let k = draw_map_key(&tc);
        let expected = self.position(k).map(|i| self.model[i].1);
        assert_eq!(self.map.get(&k).copied(), expected);
    }

    #[rule]
    fn contains(&mut self, tc: TestCase) {
        let k = draw_map_key(&tc);
        assert_eq!(self.map.contains_key(&k), self.position(k).is_some());
    }

    #[rule]
    fn remove_present(&mut self, tc: TestCase) {
        tc.assume(!self.model.is_empty());
        let i = tc.draw(gs::integers::<usize>().max_value(self.model.len() - 1));
        let (k, v) = self.model.swap_remove(i);
        assert_eq!(self.map.remove(&k), Some(v));
    }

    #[invariant]
    fn same_len(&mut self, _: TestCase) {
        assert_eq!(self.map.len(), self.model.len());
    }
}

fn machine_map(tc: TestCase) {
    machine(MapMachine::new(None)).run(tc);
}

fn shrink_machine_map(tc: TestCase) {
    machine(MapMachine::new(Some(4))).run(tc);
}

struct Counter {
    value: i64,
}

#[hegel::state_machine]
impl Counter {
    #[rule]
    fn increment(&mut self, _: TestCase) {
        self.value += 1;
    }

    #[rule]
    fn decrement(&mut self, _: TestCase) {
        self.value -= 1;
    }

    #[rule]
    fn add(&mut self, tc: TestCase) {
        self.value += tc.draw(gs::integers::<i8>()) as i64;
    }

    #[invariant]
    fn bounded(&mut self, _: TestCase) {
        assert!(self.value.abs() < 1_000_000);
    }
}

fn machine_counter(tc: TestCase) {
    machine(Counter { value: 0 }).run(tc);
}

/// A resource manager checked against a model set, the way a test of a
/// connection or file-handle pool would use [`Pool`]: `open` adds a handle,
/// `touch` draws one without consuming it, `close` consumes one.
struct HandleMachine {
    handles: Pool<u32>,
    open: std::collections::BTreeSet<u32>,
    next: u32,
    limit: Option<usize>,
}

impl HandleMachine {
    fn new(tc: &TestCase, limit: Option<usize>) -> Self {
        HandleMachine {
            handles: pool(tc),
            open: std::collections::BTreeSet::new(),
            next: 0,
            limit,
        }
    }
}

#[hegel::state_machine]
impl HandleMachine {
    #[rule]
    fn open(&mut self, tc: TestCase) {
        let stride = tc.draw(gs::integers::<u32>().min_value(1).max_value(4));
        self.next += stride;
        self.handles.add(self.next);
        self.open.insert(self.next);
        if let Some(limit) = self.limit {
            assert!(self.open.len() < limit);
        }
    }

    #[rule]
    fn touch(&mut self, tc: TestCase) {
        let h = *tc.draw(self.handles.values_reusable());
        assert!(self.open.contains(&h));
    }

    #[rule]
    fn close(&mut self, tc: TestCase) {
        let h = tc.draw(self.handles.values_consumed());
        assert!(self.open.remove(&h));
    }

    #[invariant]
    fn counts_agree(&mut self, _: TestCase) {
        assert_eq!(self.handles.len(), self.open.len());
    }
}

fn machine_pool(tc: TestCase) {
    let model = HandleMachine::new(&tc, None);
    machine(model).run(tc);
}

fn shrink_machine_pool(tc: TestCase) {
    let model = HandleMachine::new(&tc, Some(3));
    machine(model).run(tc);
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
        generate("hashmap_i32_50", hashmap_i32_50),
        generate("regex_word_digits", regex_word_digits),
        generate("machine_counter", machine_counter),
        generate("machine_map", machine_map),
        generate("machine_pool", machine_pool),
        shrink("shrink_int_above_1000", shrink_int_above_1000),
        shrink("shrink_vec_sum", shrink_vec_sum),
        shrink("shrink_text_contains", shrink_text_contains),
        shrink("shrink_vec_bool_count", shrink_vec_bool_count),
        shrink("shrink_version", shrink_version),
        shrink("shrink_tree_depth", shrink_tree_depth),
        shrink("shrink_machine_map", shrink_machine_map),
        shrink("shrink_machine_pool", shrink_machine_pool),
    ]
}
