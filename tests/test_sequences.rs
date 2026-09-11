mod common;

use common::utils::{assert_all_examples, check_can_generate_examples, minimal};
use hegel::TestCase;
use hegel::generators as gs;

fn is_subsequence<T: PartialEq>(sub: &[T], source: &[T]) -> bool {
    let mut it = source.iter();
    sub.iter().all(|x| it.any(|y| y == x))
}

fn is_sample<T: PartialEq + Clone>(sample: &[T], source: &[T]) -> bool {
    let mut pool = source.to_vec();
    sample
        .iter()
        .all(|x| match pool.iter().position(|y| y == x) {
            Some(i) => {
                pool.remove(i);
                true
            }
            None => false,
        })
}

#[test]
fn test_permutations_default() {
    check_can_generate_examples(gs::permutations(vec![1, 2, 3]));
}

#[test]
fn test_subsequences_default() {
    check_can_generate_examples(gs::subsequences(vec![1, 2, 3]));
}

#[test]
fn test_permutations_contain_all_elements() {
    assert_all_examples(gs::permutations(vec![1, 2, 3, 4]), |p: &Vec<i32>| {
        let mut sorted = p.clone();
        sorted.sort_unstable();
        sorted == vec![1, 2, 3, 4]
    });
}

#[test]
fn test_samples_default() {
    check_can_generate_examples(gs::samples(vec![1, 2, 3]));
}

#[test]
fn test_samples_min_size() {
    assert_all_examples(gs::samples(vec![1, 2, 3, 4]).min_size(2), |s: &Vec<i32>| {
        s.len() >= 2 && s.iter().all(|x| (1..=4).contains(x))
    });
}

#[test]
fn test_samples_max_size() {
    assert_all_examples(gs::samples(vec![1, 2, 3, 4]).max_size(2), |s: &Vec<i32>| {
        s.len() <= 2 && s.iter().all(|x| (1..=4).contains(x))
    });
}

#[test]
fn test_samples_with_replacement_can_repeat_elements() {
    let sample = minimal(
        gs::samples(vec![1, 2]).with_replacement().max_size(5),
        |s: &Vec<i32>| s.iter().filter(|&&x| x == 2).count() >= 2,
    );
    assert_eq!(sample, vec![2, 2]);
}

#[test]
fn test_samples_without_replacement() {
    assert_all_examples(
        gs::samples(vec![1, 2, 3, 4]).without_replacement(),
        |s: &Vec<i32>| s.len() <= 4 && is_sample(s, &[1, 2, 3, 4]),
    );
}

#[test]
fn test_samples_of_empty_sequence_are_empty() {
    assert_all_examples(gs::samples(Vec::<i32>::new()), |s: &Vec<i32>| s.is_empty());
}

#[test]
fn test_subsequences_preserve_order() {
    assert_all_examples(gs::subsequences(vec![1, 2, 3, 4, 5]), |s: &Vec<i32>| {
        s.len() <= 5 && is_subsequence(s, &[1, 2, 3, 4, 5])
    });
}

#[test]
fn test_subsequences_min_size() {
    assert_all_examples(
        gs::subsequences(vec![1, 2, 3, 4, 5]).min_size(2),
        |s: &Vec<i32>| s.len() >= 2 && is_subsequence(s, &[1, 2, 3, 4, 5]),
    );
}

#[test]
fn test_subsequences_max_size() {
    assert_all_examples(
        gs::subsequences(vec![1, 2, 3, 4, 5]).max_size(3),
        |s: &Vec<i32>| s.len() <= 3 && is_subsequence(s, &[1, 2, 3, 4, 5]),
    );
}

#[test]
fn test_permutations_in_vec() {
    assert_all_examples(
        gs::vecs(gs::permutations(vec![1, 2, 3])).max_size(5),
        |v: &Vec<Vec<i32>>| {
            v.iter().all(|p| {
                let mut sorted = p.clone();
                sorted.sort_unstable();
                sorted == vec![1, 2, 3]
            })
        },
    );
}

#[test]
fn test_samples_in_vec() {
    assert_all_examples(
        gs::vecs(gs::samples(vec![1, 2, 3]).max_size(3)).max_size(5),
        |v: &Vec<Vec<i32>>| v.iter().all(|s| s.iter().all(|x| (1..=3).contains(x))),
    );
}

#[test]
fn test_subsequences_in_vec() {
    assert_all_examples(
        gs::vecs(gs::subsequences(vec![1, 2, 3])).max_size(5),
        |v: &Vec<Vec<i32>>| v.iter().all(|s| is_subsequence(s, &[1, 2, 3])),
    );
}

#[hegel::test]
fn test_permutations_accepts_slice(tc: TestCase) {
    const NAMES: &[&str] = &["alice", "bob", "carol"];
    let perm = tc.draw(gs::permutations(NAMES));
    assert!(perm.len() == 3 && is_sample(&perm, NAMES));
}

#[hegel::test]
fn test_subsequences_accepts_slice(tc: TestCase) {
    const NAMES: &[&str] = &["alice", "bob", "carol"];
    let sub = tc.draw(gs::subsequences(NAMES));
    assert!(is_subsequence(&sub, NAMES));
}

#[hegel::test]
fn test_permutations_property(tc: TestCase) {
    let source: Vec<i32> = tc.draw(gs::vecs(gs::integers::<i32>()).max_size(10));
    let perm = tc.draw(gs::permutations(source.clone()));
    let mut expected = source;
    let mut actual = perm;
    expected.sort_unstable();
    actual.sort_unstable();
    assert_eq!(expected, actual);
}

#[hegel::test]
fn test_samples_without_replacement_size_property(tc: TestCase) {
    let source: Vec<i32> = tc.draw(gs::vecs(gs::integers::<i32>()).max_size(10));
    let min = tc.draw(gs::integers::<usize>().min_value(0).max_value(source.len()));
    let max = tc.draw(
        gs::integers::<usize>()
            .min_value(min)
            .max_value(source.len()),
    );
    let sample = tc.draw(
        gs::samples(source.clone())
            .without_replacement()
            .min_size(min)
            .max_size(max),
    );
    assert!(sample.len() >= min && sample.len() <= max);
    assert!(is_sample(&sample, &source));
}

#[hegel::test]
fn test_samples_with_replacement_size_property(tc: TestCase) {
    let source: Vec<i32> = tc.draw(gs::vecs(gs::integers::<i32>()).min_size(1).max_size(10));
    let min = tc.draw(gs::integers::<usize>().min_value(0).max_value(20));
    let max = tc.draw(gs::integers::<usize>().min_value(min).max_value(20));
    let sample = tc.draw(gs::samples(source.clone()).min_size(min).max_size(max));
    assert!(sample.len() >= min && sample.len() <= max);
    assert!(sample.iter().all(|x| source.contains(x)));
}

#[hegel::test]
fn test_subsequences_size_property(tc: TestCase) {
    let source: Vec<i32> = tc.draw(gs::vecs(gs::integers::<i32>()).max_size(10));
    let min = tc.draw(gs::integers::<usize>().min_value(0).max_value(source.len()));
    let max = tc.draw(
        gs::integers::<usize>()
            .min_value(min)
            .max_value(source.len()),
    );
    let sub = tc.draw(gs::subsequences(source.clone()).min_size(min).max_size(max));
    assert!(sub.len() >= min && sub.len() <= max);
    assert!(is_subsequence(&sub, &source));
}

#[test]
fn test_permutations_shrink_to_original_order() {
    let perm = minimal(gs::permutations(vec![3, 1, 2]), |_| true);
    assert_eq!(perm, vec![3, 1, 2]);
}

#[test]
fn test_permutations_shrink_moves_one_element() {
    let perm = minimal(gs::permutations(vec![1, 2, 3, 4, 5]), |p: &Vec<i32>| {
        p[0] == 5
    });
    assert_eq!(perm, vec![5, 1, 2, 3, 4]);
}

#[test]
fn test_subsequences_shrink_to_empty() {
    let sub = minimal(gs::subsequences(vec![1, 2, 3, 4, 5]), |_| true);
    assert!(sub.is_empty());
}

#[test]
fn test_subsequences_shrink_to_prefix() {
    let sub = minimal(gs::subsequences(vec![1, 2, 3, 4, 5]).min_size(3), |_| true);
    assert_eq!(sub, vec![1, 2, 3]);
}

#[test]
fn test_samples_shrink_to_empty() {
    let sample = minimal(gs::samples(vec![1, 2, 3]).max_size(5), |_| true);
    assert!(sample.is_empty());
}

#[test]
fn test_samples_with_replacement_shrink_to_repeated_first_element() {
    let sample = minimal(gs::samples(vec![1, 2, 3]).min_size(3).max_size(5), |_| true);
    assert_eq!(sample, vec![1, 1, 1]);
}

#[test]
fn test_samples_without_replacement_shrink_to_prefix() {
    let sample = minimal(
        gs::samples(vec![1, 2, 3, 4, 5])
            .without_replacement()
            .min_size(3),
        |_| true,
    );
    assert_eq!(sample, vec![1, 2, 3]);
}
