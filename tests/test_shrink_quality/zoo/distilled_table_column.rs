//! Control: a table with a header, where an irrelevant column can only go if it goes from every
//! row at once.
//!
//! `header = vecs(integers 0..=1000).min_size(1).max_size(3)`,
//! `rows = vecs(vecs(…).max_size(3)).min_size(1).max_size(3)`; the property is only checked when
//! every row is as wide as the header and fails iff some cell is non-zero. Shortlex ideal
//! `([0], [[1]])`. The rows are drawn free and checked in the property, as a naive generator
//! would, so a column deletion is one non-contiguous deletion per row plus one in the header. A
//! human writes the same.

use super::assert_shrinks_to;
use hegel::TestCase;
use hegel::generators as gs;

type Draws = (Vec<i64>, Vec<Vec<i64>>);

fn cells() -> gs::IntegerGenerator<i64> {
    gs::integers::<i64>().min_value(0).max_value(1000)
}

fn draw(tc: &TestCase) -> Draws {
    let header: Vec<i64> = tc.draw_silent(gs::vecs(cells()).min_size(1).max_size(3));
    let rows: Vec<Vec<i64>> = tc.draw_silent(
        gs::vecs(gs::vecs(cells()).max_size(3))
            .min_size(1)
            .max_size(3),
    );
    (header, rows)
}

fn well_formed_with_payload((header, rows): &Draws) -> bool {
    rows.iter().all(|r| r.len() == header.len()) && rows.iter().flatten().any(|&x| x != 0)
}

#[test]
fn the_ideal_fails_and_is_smallest() {
    assert!(well_formed_with_payload(&(vec![0], vec![vec![1]])));
    assert!(!well_formed_with_payload(&(vec![0], vec![vec![0]])));
    assert!(!well_formed_with_payload(&(vec![0], vec![vec![1, 0]])));
    assert!(!well_formed_with_payload(&(vec![0, 0], vec![vec![1]])));
}

#[test]
fn control_irrelevant_column_is_deleted_from_header_and_rows() {
    assert_shrinks_to(
        &(vec![0], vec![vec![1]]),
        30,
        200,
        draw,
        well_formed_with_payload,
    );
}
