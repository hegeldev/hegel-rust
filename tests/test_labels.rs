//! `Generator::label`: every generator has a stable label, generators built
//! from others fold their components' labels in, and wrappers that change
//! nothing about what is drawn keep the wrapped generator's label.

use hegel::generators::{self as gs, DefaultGenerator as _, Generator, PrintableGenerator};
use hegel::{DefaultGenerator, Hegel, PrettyPrintable, Settings};

fn ints() -> gs::IntegerGenerator<i32> {
    gs::integers::<i32>()
}

#[test]
fn the_same_generator_always_has_the_same_label() {
    assert_eq!(ints().label(), ints().label());
    assert_eq!(gs::text().label(), gs::text().label());
    assert_eq!(
        gs::vecs(ints()).max_size(3).label(),
        gs::vecs(ints()).label()
    );
}

#[test]
fn leaf_generators_of_different_types_differ() {
    assert_ne!(ints().label(), gs::integers::<i64>().label());
    assert_ne!(ints().label(), gs::text().label());
    assert_ne!(gs::booleans().label(), gs::floats::<f64>().label());
}

#[test]
fn collections_fold_in_their_element_labels() {
    assert_ne!(gs::vecs(ints()).label(), gs::vecs(gs::text()).label());
    assert_ne!(gs::vecs(ints()).label(), ints().label());
    assert_ne!(gs::vecs(ints()).label(), gs::hashsets(ints()).label());
    assert_ne!(gs::hashsets(ints()).label(), gs::btree_sets(ints()).label());
    assert_ne!(
        gs::hashsets(ints()).label(),
        gs::hashsets(gs::text()).label()
    );
    assert_ne!(
        gs::btree_sets(ints()).label(),
        gs::btree_sets(gs::text()).label()
    );
    assert_ne!(
        gs::hashmaps(ints(), gs::text()).label(),
        gs::hashmaps(gs::text(), ints()).label()
    );
    assert_ne!(
        gs::btree_maps(ints(), gs::text()).label(),
        gs::btree_maps(ints(), ints()).label()
    );
    assert_ne!(
        gs::hashmaps(ints(), ints()).label(),
        gs::btree_maps(ints(), ints()).label()
    );
    assert_ne!(
        gs::arrays::<_, _, 3>(ints()).label(),
        gs::arrays::<_, _, 3>(gs::text()).label()
    );
    assert_eq!(
        gs::arrays::<_, _, 3>(ints()).label(),
        gs::arrays::<_, _, 4>(ints()).label()
    );
}

#[test]
fn tuples_fold_in_their_component_labels() {
    assert_eq!(
        hegel::tuples!(ints(), gs::text()).label(),
        hegel::tuples!(ints(), gs::text()).label()
    );
    assert_ne!(
        hegel::tuples!(ints(), gs::text()).label(),
        hegel::tuples!(gs::text(), ints()).label()
    );
    assert_ne!(
        hegel::tuples!(ints()).label(),
        hegel::tuples!(ints(), ints()).label()
    );
    assert_eq!(hegel::tuples!().label(), hegel::tuples!().label());
}

#[test]
fn value_transforming_combinators_differ_from_their_source() {
    let source = ints();
    let mapped = ints().map(|n| n + 1);
    let filtered = ints().filter(|n| n % 2 == 0);
    let flat_mapped = ints().flat_map(|_| ints());
    let labels = [
        source.label(),
        mapped.label(),
        filtered.label(),
        flat_mapped.label(),
    ];
    for (i, a) in labels.iter().enumerate() {
        for b in &labels[i + 1..] {
            assert_ne!(a, b);
        }
    }
    assert_eq!(mapped.label(), ints().map(|n| n * 2).label());
    assert_ne!(mapped.label(), gs::text().map(|s| s.len() as i32).label());
    assert_ne!(
        filtered.label(),
        gs::text().filter(|s| s.is_empty()).label()
    );
    assert_ne!(flat_mapped.label(), gs::text().flat_map(|_| ints()).label());
    assert_eq!(
        ints().map(|n| n + 1).print_as_call("inc").label(),
        mapped.label()
    );
}

#[test]
fn one_of_and_optional_fold_in_their_alternatives() {
    let a = || ints();
    let b = || ints().map(|n| n * 2);
    assert_eq!(
        hegel::one_of!(a(), b()).label(),
        hegel::one_of!(a(), b()).label()
    );
    assert_ne!(
        hegel::one_of!(a(), b()).label(),
        hegel::one_of!(b(), a()).label()
    );
    assert_ne!(hegel::one_of!(a()).label(), a().label());
    assert_eq!(
        gs::one_of(vec![a().boxed(), b().boxed()]).label(),
        gs::one_of(vec![a().boxed(), b().boxed()]).label()
    );
    assert_ne!(
        gs::one_of(vec![a().boxed(), b().boxed()]).label(),
        gs::one_of(vec![a().boxed()]).label()
    );
    assert_ne!(gs::optional(a()).label(), gs::optional(b()).label());
    assert_ne!(gs::optional(a()).label(), a().label());
    assert_ne!(gs::ip_addresses().label(), gs::ip_addresses().v4().label());
}

struct Coin;

impl gs::Alternatives<i32> for Coin {
    fn max_index(&self) -> usize {
        1
    }
    fn draw_at(&self, tc: &hegel::TestCase, index: usize) -> i32 {
        tc.draw(gs::just(index as i32))
    }
}

#[test]
fn custom_alternatives_get_a_label_from_their_type() {
    let coin = gs::one_of_from_alternatives(Coin);
    assert_eq!(coin.label(), gs::one_of_from_alternatives(Coin).label());
    assert_ne!(
        coin.label(),
        hegel::one_of!(gs::just(0), gs::just(1)).label()
    );
}

#[test]
fn wrappers_that_change_nothing_keep_the_wrapped_label() {
    fn label_of<G: Generator<i32>>(generator: G) -> u64 {
        generator.label()
    }
    let inner = ints().map(|n| n + 1);
    let expected = inner.label();
    assert_eq!(label_of(&inner), expected);
    assert_eq!(ints().map(|n| n + 1).boxed().label(), expected);
    assert_eq!(ints().map(|n| n + 1).boxed_printable().label(), expected);
    assert_eq!(ints().map(|n| n + 1).print_as_value().label(), expected);
    assert_eq!(ints().map(|n| n + 1).print_as_debug().label(), expected);
    assert_eq!(
        ints()
            .map(|n| n + 1)
            .print_with(|_, printer| printer.text("?"))
            .label(),
        expected
    );

    let deferred = gs::deferred::<i32>();
    let handle = deferred.generator();
    deferred.set(ints().map(|n| n + 1));
    assert_eq!(handle.label(), expected);

    let silent = gs::deferred_silent::<i32>();
    let handle = silent.generator();
    silent.set(ints().map(|n| n + 1));
    assert_eq!(handle.label(), expected);
}

#[derive(Debug, Clone, PartialEq, PrettyPrintable)]
enum Tree {
    Leaf(i32),
    Branch(Box<Tree>, Box<Tree>),
}

fn tree() -> gs::BoxedPrintableGenerator<'static, Tree> {
    let tree = gs::deferred::<Tree>();
    let handle = tree.generator();
    let leaf = ints().map(Tree::Leaf);
    let branch = hegel::tuples!(tree.generator(), tree.generator())
        .map(|(l, r)| Tree::Branch(Box::new(l), Box::new(r)));
    tree.set(hegel::one_of!(leaf, branch));
    handle
}

#[test]
fn self_referential_deferred_definitions_have_a_label() {
    let a = tree();
    assert_eq!(a.label(), a.label());
    assert_eq!(a.label(), tree().label());
    assert_ne!(a.label(), ints().map(Tree::Leaf).label());

    let other = gs::deferred::<Tree>();
    let handle = other.generator();
    let branch = hegel::tuples!(other.generator(), other.generator())
        .map(|(l, r)| Tree::Branch(Box::new(l), Box::new(r)));
    other.set(hegel::one_of!(gs::just(Tree::Leaf(0)), branch));
    assert_ne!(handle.label(), a.label());

    let unset = gs::deferred::<Tree>();
    let unset_handle = unset.generator();
    let inner = hegel::tuples!(unset.generator(), a.clone())
        .map(|(l, r)| Tree::Branch(Box::new(l), Box::new(r)));
    unset.set(inner);
    assert_ne!(unset_handle.label(), a.label());
}

#[test]
fn recursive_generators_label_every_subtree_alike() {
    let recursive = || {
        gs::recursive(ints().map(|n| vec![n]), |subtrees| {
            gs::vecs(subtrees).max_size(3).map(|vs| vs.concat())
        })
    };
    assert_eq!(recursive().label(), recursive().label());
    assert_ne!(recursive().label(), ints().map(|n| vec![n]).label());
    assert_ne!(
        recursive().label(),
        gs::recursive(gs::text().map(|s| vec![s.len() as i32]), |subtrees| {
            gs::vecs(subtrees).max_size(3).map(|vs| vs.concat())
        })
        .label()
    );

    let outer = recursive().label();
    Hegel::new(move |tc| {
        tc.draw(gs::recursive(ints().map(|n| vec![n]), move |subtrees| {
            assert_eq!(subtrees.label(), outer);
            assert_eq!(subtrees.clone().label(), outer);
            gs::vecs(subtrees).max_size(3).map(|vs| vs.concat())
        }));
    })
    .settings(Settings::new().database(None).test_cases(5))
    .run();
}

#[test]
fn sequence_generators_have_fixed_labels() {
    let elements = vec![1, 2, 3];
    assert_eq!(
        gs::subsequences(elements.clone()).label(),
        gs::subsequences(vec!["a"]).label()
    );
    assert_eq!(
        gs::permutations(elements.clone()).label(),
        gs::permutations(vec!["a"]).label()
    );
    assert_eq!(
        gs::samples(elements.clone()).label(),
        gs::samples(vec!["a"]).without_replacement().label()
    );
    assert_ne!(
        gs::subsequences(elements.clone()).label(),
        gs::permutations(elements.clone()).label()
    );
    assert_ne!(
        gs::permutations(elements.clone()).label(),
        gs::samples(elements).label()
    );
}

#[test]
fn composites_are_labelled_by_their_source() {
    let same_a = hegel::compose!(|tc| { tc.draw(gs::integers::<i32>()) });
    let same_b = hegel::compose!(|tc| { tc.draw(gs::integers::<i32>()) });
    let different = hegel::compose!(|tc| { tc.draw(gs::integers::<i32>()) + 1 });
    assert_eq!(same_a.label(), same_b.label());
    assert_ne!(same_a.label(), different.label());
}

#[derive(Debug, Clone, PartialEq, DefaultGenerator, PrettyPrintable)]
struct Point {
    x: i32,
    y: i32,
}

#[derive(Debug, Clone, PartialEq, DefaultGenerator, PrettyPrintable)]
struct Pair(i32, bool);

#[derive(Debug, Clone, PartialEq, DefaultGenerator, PrettyPrintable)]
enum Shape {
    Dot,
    Circle { radius: i32 },
    Line(i32, i32),
}

#[test]
fn derived_generators_fold_in_their_field_generators() {
    assert_eq!(
        Point::default_generator().label(),
        Point::default_generator().label()
    );
    assert_ne!(
        Point::default_generator().label(),
        Pair::default_generator().label()
    );
    assert_ne!(
        Point::default_generator().label(),
        Point::default_generator()
            .x(ints().map(|n| n.wrapping_abs()))
            .label()
    );
    assert_ne!(
        Pair::default_generator().label(),
        Pair::default_generator()
            ._1(gs::booleans().map(|b| !b))
            .label()
    );
    assert_eq!(
        Shape::default_generator().label(),
        Shape::default_generator().label()
    );
    assert_ne!(
        Shape::default_generator().label(),
        Shape::default_generator()
            .circle(|circle| circle.radius(ints().map(|n| n.wrapping_abs())))
            .label()
    );
    assert_ne!(
        Shape::default_generator().label(),
        Shape::default_generator()
            .line(ints().map(|n| n.wrapping_abs()), ints())
            .label()
    );
}

#[test]
fn labels_from_names_are_stable_and_distinct() {
    assert_eq!(
        gs::label_from_name("mycrate.pairs"),
        gs::label_from_name("mycrate.pairs")
    );
    assert_ne!(
        gs::label_from_name("mycrate.pairs"),
        gs::label_from_name("mycrate.triples")
    );
    let pairs = gs::label_from_name("mycrate.pairs");
    assert_ne!(
        gs::combine_labels(&[pairs, ints().label()]),
        gs::combine_labels(&[pairs, gs::text().label()])
    );
    assert_ne!(gs::combine_labels(&[pairs]), pairs);
}
