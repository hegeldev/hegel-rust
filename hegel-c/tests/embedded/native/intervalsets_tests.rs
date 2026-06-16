use super::*;

#[test]
fn new_concatenates_offsets() {
    let iv = IntervalSet::new(vec![(0, 2), (10, 12)]);
    assert_eq!(iv.len(), 6);
    assert_eq!(iv.get(0), Some(0));
    assert_eq!(iv.get(2), Some(2));
    assert_eq!(iv.get(3), Some(10));
    assert_eq!(iv.get(5), Some(12));
}

#[test]
fn get_out_of_range_returns_none() {
    let iv = IntervalSet::new(vec![(0, 2)]);
    assert!(iv.get(3).is_none());
    assert!(iv.get(-4).is_none());
}

#[test]
fn get_negative_index_resolves_from_end() {
    let iv = IntervalSet::new(vec![(5, 9)]);
    assert_eq!(iv.get(-1), Some(9));
    assert_eq!(iv.get(-5), Some(5));
}

#[test]
fn index_returns_position_when_present() {
    let iv = IntervalSet::new(vec![(10, 12), (20, 22)]);
    assert_eq!(iv.index(10), Some(0));
    assert_eq!(iv.index(11), Some(1));
    assert_eq!(iv.index(20), Some(3));
    assert_eq!(iv.index(22), Some(5));
}

#[test]
fn index_returns_none_between_intervals() {
    // value falls in the gap between two intervals — the `u > value` arm.
    let iv = IntervalSet::new(vec![(0, 5), (10, 15)]);
    assert!(iv.index(7).is_none());
}

#[test]
fn index_returns_none_past_all_intervals() {
    let iv = IntervalSet::new(vec![(0, 5)]);
    assert!(iv.index(100).is_none());
}

#[test]
fn union_with_empty_other_returns_self() {
    let a = IntervalSet::new(vec![(0, 5)]);
    let b = IntervalSet::new(Vec::new());
    let u = a.union(&b);
    assert_eq!(u, a);
}

#[test]
fn union_with_empty_self_returns_other() {
    let a = IntervalSet::new(Vec::new());
    let b = IntervalSet::new(vec![(0, 5)]);
    let u = a.union(&b);
    assert_eq!(u, b);
}

#[test]
fn union_merges_overlapping_intervals() {
    let a = IntervalSet::new(vec![(0, 5), (20, 25)]);
    let b = IntervalSet::new(vec![(3, 10), (24, 30)]);
    let u = a.union(&b);
    assert_eq!(u, IntervalSet::new(vec![(0, 10), (20, 30)]));
}

#[test]
fn union_merges_adjacent_intervals() {
    let a = IntervalSet::new(vec![(0, 5)]);
    let b = IntervalSet::new(vec![(6, 10)]);
    let u = a.union(&b);
    assert_eq!(u, IntervalSet::new(vec![(0, 10)]));
}

#[test]
fn difference_advances_past_smaller_other_interval() {
    // `other` interval ends before any `self` interval begins — exercises
    // the `yr < xl` advance branch.
    let a = IntervalSet::new(vec![(20, 30)]);
    let b = IntervalSet::new(vec![(0, 5)]);
    let d = a.difference(&b);
    assert_eq!(d, a);
}

#[test]
fn difference_carves_middle() {
    let a = IntervalSet::new(vec![(0, 10)]);
    let b = IntervalSet::new(vec![(4, 6)]);
    let d = a.difference(&b);
    assert_eq!(d, IntervalSet::new(vec![(0, 3), (7, 10)]));
}

#[test]
fn intersection_advances_past_smaller_self_interval() {
    // `self` interval ends before any `other` interval begins — exercises
    // the `uu > v` advance branch.
    let a = IntervalSet::new(vec![(0, 5)]);
    let b = IntervalSet::new(vec![(20, 30)]);
    let r = a.intersection(&b);
    assert_eq!(r, IntervalSet::new(Vec::new()));
}

#[test]
fn intersection_overlapping_intervals() {
    let a = IntervalSet::new(vec![(0, 10), (20, 30)]);
    let b = IntervalSet::new(vec![(5, 25)]);
    let r = a.intersection(&b);
    assert_eq!(r, IntervalSet::new(vec![(5, 10), (20, 25)]));
}

#[test]
fn char_in_shrink_order_round_trips_via_index() {
    let iv = IntervalSet::new(vec![(0, 0xD7FF), (0xE000, 0x10FFFF)]);
    for c in ['0', '1', '9', 'A', 'Z', 'a', '/', '\0', '~'] {
        let idx = iv.index_from_char_in_shrink_order(c);
        assert_eq!(iv.char_in_shrink_order(idx), c);
    }
}
