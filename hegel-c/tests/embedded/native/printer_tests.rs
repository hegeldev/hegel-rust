use super::*;

use rand::rngs::SmallRng;
use rand::seq::SliceRandom;
use rand::{RngExt, SeedableRng};

const M: Target = Target::Main;

fn printer(max_width: usize) -> Printer {
    Printer::new(max_width)
}

#[test]
fn plain_text_passes_through() {
    let mut p = printer(79);
    p.text(M, "hello").unwrap();
    p.text(M, " world").unwrap();
    assert_eq!(p.value().unwrap(), "hello world");
}

#[test]
fn fitting_group_renders_separators_inline() {
    let mut p = printer(20);
    p.begin_group(M, 1, "[").unwrap();
    p.text(M, "1").unwrap();
    p.text(M, ",").unwrap();
    p.breakable(M, " ").unwrap();
    p.text(M, "2").unwrap();
    p.text(M, ",").unwrap();
    p.breakable(M, " ").unwrap();
    p.text(M, "3").unwrap();
    p.end_group(M, "]").unwrap();
    assert_eq!(p.value().unwrap(), "[1, 2, 3]");
}

#[test]
fn overflowing_group_breaks_every_breakable() {
    let mut p = printer(6);
    p.begin_group(M, 1, "[").unwrap();
    p.text(M, "1").unwrap();
    p.text(M, ",").unwrap();
    p.breakable(M, " ").unwrap();
    p.text(M, "2").unwrap();
    p.text(M, ",").unwrap();
    p.breakable(M, " ").unwrap();
    p.text(M, "3").unwrap();
    p.end_group(M, "]").unwrap();
    assert_eq!(p.value().unwrap(), "[1,\n 2,\n 3]");
}

#[test]
fn nested_group_stays_inline_when_outer_breaks() {
    let mut p = printer(10);
    p.begin_group(M, 1, "[").unwrap();
    p.begin_group(M, 1, "[").unwrap();
    p.text(M, "1,").unwrap();
    p.breakable(M, " ").unwrap();
    p.text(M, "2").unwrap();
    p.end_group(M, "]").unwrap();
    p.text(M, ",").unwrap();
    p.breakable(M, " ").unwrap();
    p.begin_group(M, 1, "[").unwrap();
    p.text(M, "3,").unwrap();
    p.breakable(M, " ").unwrap();
    p.text(M, "4").unwrap();
    p.end_group(M, "]").unwrap();
    p.end_group(M, "]").unwrap();
    assert_eq!(p.value().unwrap(), "[[1, 2],\n [3, 4]]");
}

#[test]
fn breakable_in_already_broken_group_breaks_immediately() {
    let mut p = printer(5);
    p.begin_group(M, 0, "").unwrap();
    p.text(M, "aaaa").unwrap();
    p.breakable(M, " ").unwrap();
    p.text(M, "bb").unwrap();
    p.breakable(M, " ").unwrap();
    p.text(M, "cc").unwrap();
    p.end_group(M, "").unwrap();
    assert_eq!(p.value().unwrap(), "aaaa\nbb\ncc");
}

#[test]
fn hard_break_applies_current_indentation() {
    let mut p = printer(79);
    p.text(M, "a").unwrap();
    p.shift_indent(M, 4).unwrap();
    p.hard_break(M).unwrap();
    p.text(M, "b").unwrap();
    assert_eq!(p.value().unwrap(), "a\n    b");
}

#[test]
fn call_style_layout_with_group_indent() {
    let mut p = printer(79);
    p.begin_group(M, 4, "f(").unwrap();
    p.hard_break(M).unwrap();
    p.text(M, "x,").unwrap();
    p.end_group(M, "").unwrap();
    p.hard_break(M).unwrap();
    p.text(M, ")").unwrap();
    assert_eq!(p.value().unwrap(), "f(\n    x,\n)");
}

#[test]
fn negative_indentation_renders_no_spaces_and_recovers() {
    let mut p = printer(79);
    p.shift_indent(M, -3).unwrap();
    p.hard_break(M).unwrap();
    p.text(M, "a").unwrap();
    p.shift_indent(M, 5).unwrap();
    p.hard_break(M).unwrap();
    p.text(M, "b").unwrap();
    assert_eq!(p.value().unwrap(), "\na\n  b");
}

#[test]
fn exact_fit_does_not_break() {
    let mut p = printer(5);
    p.text(M, "12").unwrap();
    p.breakable(M, " ").unwrap();
    p.text(M, "45").unwrap();
    assert_eq!(p.value().unwrap(), "12 45");
}

#[test]
fn one_over_max_width_breaks_at_top_level() {
    let mut p = printer(4);
    p.text(M, "12").unwrap();
    p.breakable(M, " ").unwrap();
    p.text(M, "45").unwrap();
    assert_eq!(p.value().unwrap(), "12\n45");
}

#[test]
fn width_is_counted_in_chars_not_bytes() {
    let mut p = printer(5);
    p.text(M, "éé").unwrap();
    p.breakable(M, " ").unwrap();
    p.text(M, "éé").unwrap();
    assert_eq!(p.value().unwrap(), "éé éé");

    let mut p = printer(4);
    p.text(M, "éé").unwrap();
    p.breakable(M, " ").unwrap();
    p.text(M, "éé").unwrap();
    assert_eq!(p.value().unwrap(), "éé\néé");
}

#[test]
fn buffered_text_coalesces_after_breakable() {
    let mut p = printer(79);
    p.text(M, "a").unwrap();
    p.breakable(M, " ").unwrap();
    p.text(M, "b").unwrap();
    p.text(M, "c").unwrap();
    assert_eq!(p.value().unwrap(), "a bc");
}

#[test]
fn end_group_without_begin_is_an_error() {
    let mut p = printer(79);
    assert_eq!(p.end_group(M, ""), Err(PrinterError::UnbalancedGroup));
    p.begin_group(M, 0, "").unwrap();
    p.end_group(M, "").unwrap();
    assert_eq!(p.end_group(M, ""), Err(PrinterError::UnbalancedGroup));
}

#[test]
fn closing_a_group_whose_breakables_were_flushed_is_fine() {
    let mut p = printer(79);
    p.begin_group(M, 0, "").unwrap();
    p.breakable(M, " ").unwrap();
    p.hard_break(M).unwrap();
    p.end_group(M, "").unwrap();
    assert_eq!(p.value().unwrap(), " \n");
}

#[test]
fn overflow_without_remaining_breakables_is_tolerated() {
    let mut p = printer(3);
    p.text(M, "ab").unwrap();
    p.breakable(M, " ").unwrap();
    p.text(M, "cdefg").unwrap();
    assert_eq!(p.value().unwrap(), "ab\ncdefg");

    let mut p = printer(3);
    p.text(M, "abcdef").unwrap();
    assert_eq!(p.value().unwrap(), "abcdef");
}

#[test]
fn deq_marks_groups_without_breakables_for_breaking() {
    let mut p = printer(4);
    p.begin_group(M, 0, "").unwrap();
    p.begin_group(M, 0, "").unwrap();
    p.text(M, "aaaa").unwrap();
    p.breakable(M, " ").unwrap();
    p.text(M, "bb").unwrap();
    p.end_group(M, "").unwrap();
    p.breakable(M, " ").unwrap();
    p.text(M, "cc").unwrap();
    p.end_group(M, "").unwrap();
    assert_eq!(p.value().unwrap(), "aaaa\nbb\ncc");
}

#[test]
fn deferred_content_is_spliced_at_the_hole() {
    let mut p = printer(79);
    p.text(M, "a").unwrap();
    let slot = p.deferred(M).unwrap();
    p.text(M, "c").unwrap();
    p.text(Target::Slot(slot), "b").unwrap();
    p.resolve().unwrap();
    assert_eq!(p.value().unwrap(), "abc");
}

#[test]
fn sequential_deferreds_fill_independently() {
    let mut p = printer(79);
    let first = p.deferred(M).unwrap();
    p.text(M, "-").unwrap();
    let second = p.deferred(M).unwrap();
    p.text(Target::Slot(second), "2").unwrap();
    p.text(Target::Slot(first), "1").unwrap();
    p.resolve().unwrap();
    assert_eq!(p.value().unwrap(), "1-2");
}

#[test]
fn deferreds_nest_inside_slots() {
    let mut p = printer(79);
    let outer = p.deferred(M).unwrap();
    p.text(M, "!").unwrap();
    p.text(Target::Slot(outer), "x").unwrap();
    let inner = p.deferred(Target::Slot(outer)).unwrap();
    p.text(Target::Slot(outer), "z").unwrap();
    p.text(Target::Slot(inner), "y").unwrap();
    p.resolve().unwrap();
    assert_eq!(p.value().unwrap(), "xyz!");
}

#[test]
fn opening_a_deferred_does_not_force_a_premature_break() {
    let mut inline = printer(10);
    inline.begin_group(M, 1, "[").unwrap();
    inline.text(M, "123").unwrap();
    inline.breakable(M, " ").unwrap();
    inline.text(M, "45678").unwrap();
    inline.end_group(M, "]").unwrap();

    let mut p = printer(10);
    p.begin_group(M, 1, "[").unwrap();
    p.text(M, "123").unwrap();
    p.breakable(M, " ").unwrap();
    let slot = p.deferred(M).unwrap();
    p.end_group(M, "]").unwrap();
    p.text(Target::Slot(slot), "45678").unwrap();
    p.resolve().unwrap();

    assert_eq!(p.value().unwrap(), inline.value().unwrap());
}

#[test]
fn slot_dies_after_resolve() {
    let mut p = printer(79);
    let slot = p.deferred(M).unwrap();
    assert!(p.slot_is_live(slot));
    p.resolve().unwrap();
    assert!(!p.slot_is_live(slot));
    assert_eq!(p.text(Target::Slot(slot), "x"), Err(PrinterError::DeadSlot));
    assert_eq!(
        p.breakable(Target::Slot(slot), " "),
        Err(PrinterError::DeadSlot)
    );
    assert_eq!(
        p.hard_break(Target::Slot(slot)),
        Err(PrinterError::DeadSlot)
    );
    assert_eq!(
        p.begin_group(Target::Slot(slot), 0, ""),
        Err(PrinterError::DeadSlot)
    );
    assert_eq!(
        p.end_group(Target::Slot(slot), ""),
        Err(PrinterError::DeadSlot)
    );
    assert_eq!(
        p.shift_indent(Target::Slot(slot), 1),
        Err(PrinterError::DeadSlot)
    );
    assert_eq!(p.deferred(Target::Slot(slot)), Err(PrinterError::DeadSlot));
    assert_eq!(
        p.begin_speculative(Target::Slot(slot)),
        Err(PrinterError::DeadSlot)
    );
    assert_eq!(
        p.commit_speculative(Target::Slot(slot)),
        Err(PrinterError::DeadSlot)
    );
    assert_eq!(
        p.abort_speculative(Target::Slot(slot)),
        Err(PrinterError::DeadSlot)
    );
}

#[test]
fn resolve_without_outstanding_deferreds_is_an_error() {
    let mut p = printer(79);
    assert_eq!(p.resolve(), Err(PrinterError::NothingToResolve));
    p.deferred(M).unwrap();
    p.resolve().unwrap();
    assert_eq!(p.resolve(), Err(PrinterError::NothingToResolve));
}

#[test]
fn a_new_session_can_start_after_resolve() {
    let mut p = printer(79);
    let first = p.deferred(M).unwrap();
    p.text(Target::Slot(first), "a").unwrap();
    p.resolve().unwrap();
    let second = p.deferred(M).unwrap();
    p.text(Target::Slot(second), "b").unwrap();
    p.resolve().unwrap();
    assert_eq!(p.value().unwrap(), "ab");
}

#[test]
fn unfilled_deferred_splices_nothing() {
    let mut p = printer(79);
    p.text(M, "a").unwrap();
    p.deferred(M).unwrap();
    p.text(M, "b").unwrap();
    p.resolve().unwrap();
    assert_eq!(p.value().unwrap(), "ab");
}

#[test]
fn value_with_unresolved_deferred_is_an_error() {
    let mut p = printer(79);
    p.deferred(M).unwrap();
    assert_eq!(p.value(), Err(PrinterError::UnresolvedDeferred));
}

#[test]
fn resolve_and_value_require_speculation_to_be_closed() {
    let mut p = printer(79);
    p.begin_speculative(M).unwrap();
    assert_eq!(p.resolve(), Err(PrinterError::OpenSpeculation));
    assert_eq!(p.value(), Err(PrinterError::OpenSpeculation));

    let mut p = printer(79);
    let slot = p.deferred(M).unwrap();
    p.begin_speculative(Target::Slot(slot)).unwrap();
    assert_eq!(p.resolve(), Err(PrinterError::OpenSpeculation));
}

#[test]
fn committed_speculation_appears_in_output() {
    let mut p = printer(79);
    p.text(M, "a").unwrap();
    p.begin_speculative(M).unwrap();
    p.text(M, "b").unwrap();
    p.commit_speculative(M).unwrap();
    assert_eq!(p.value().unwrap(), "ab");
}

#[test]
fn aborted_speculation_is_dropped() {
    let mut p = printer(79);
    p.text(M, "a").unwrap();
    p.begin_speculative(M).unwrap();
    p.text(M, "b").unwrap();
    p.breakable(M, " ").unwrap();
    p.hard_break(M).unwrap();
    p.abort_speculative(M).unwrap();
    p.text(M, "c").unwrap();
    assert_eq!(p.value().unwrap(), "ac");
}

#[test]
fn nested_speculation_commit_then_abort_drops_both() {
    let mut p = printer(79);
    p.text(M, "a").unwrap();
    p.begin_speculative(M).unwrap();
    p.text(M, "b").unwrap();
    p.begin_speculative(M).unwrap();
    p.text(M, "c").unwrap();
    p.commit_speculative(M).unwrap();
    p.abort_speculative(M).unwrap();
    assert_eq!(p.value().unwrap(), "a");
}

#[test]
fn nested_speculation_abort_then_commit_keeps_outer() {
    let mut p = printer(79);
    p.text(M, "a").unwrap();
    p.begin_speculative(M).unwrap();
    p.text(M, "b").unwrap();
    p.begin_speculative(M).unwrap();
    p.text(M, "c").unwrap();
    p.abort_speculative(M).unwrap();
    p.commit_speculative(M).unwrap();
    assert_eq!(p.value().unwrap(), "ab");
}

#[test]
fn speculation_while_recording_appends_to_the_session() {
    let mut p = printer(79);
    p.text(M, "a").unwrap();
    let slot = p.deferred(M).unwrap();
    p.begin_speculative(M).unwrap();
    p.text(M, "c").unwrap();
    p.commit_speculative(M).unwrap();
    p.text(Target::Slot(slot), "b").unwrap();
    p.resolve().unwrap();
    assert_eq!(p.value().unwrap(), "abc");
}

#[test]
fn speculation_on_a_slot_commits_into_the_slot() {
    let mut p = printer(79);
    p.text(M, "a").unwrap();
    let slot = p.deferred(M).unwrap();
    p.begin_speculative(Target::Slot(slot)).unwrap();
    p.text(Target::Slot(slot), "b").unwrap();
    p.commit_speculative(Target::Slot(slot)).unwrap();
    p.text(Target::Slot(slot), "c").unwrap();
    p.resolve().unwrap();
    assert_eq!(p.value().unwrap(), "abc");
}

#[test]
fn speculation_on_a_slot_aborts_cleanly() {
    let mut p = printer(79);
    p.text(M, "a").unwrap();
    let slot = p.deferred(M).unwrap();
    p.begin_speculative(Target::Slot(slot)).unwrap();
    p.text(Target::Slot(slot), "b").unwrap();
    p.abort_speculative(Target::Slot(slot)).unwrap();
    p.text(Target::Slot(slot), "c").unwrap();
    p.resolve().unwrap();
    assert_eq!(p.value().unwrap(), "ac");
}

#[test]
fn nested_slot_speculation_commits_into_outer_buffer() {
    let mut p = printer(79);
    let slot = p.deferred(M).unwrap();
    p.begin_speculative(Target::Slot(slot)).unwrap();
    p.text(Target::Slot(slot), "a").unwrap();
    p.begin_speculative(Target::Slot(slot)).unwrap();
    p.text(Target::Slot(slot), "b").unwrap();
    p.commit_speculative(Target::Slot(slot)).unwrap();
    p.commit_speculative(Target::Slot(slot)).unwrap();
    p.resolve().unwrap();
    assert_eq!(p.value().unwrap(), "ab");
}

#[test]
fn aborting_speculation_kills_deferreds_opened_inside_it() {
    let mut p = printer(79);
    p.begin_speculative(M).unwrap();
    let slot = p.deferred(M).unwrap();
    p.abort_speculative(M).unwrap();
    assert!(!p.slot_is_live(slot));
    assert_eq!(p.text(Target::Slot(slot), "x"), Err(PrinterError::DeadSlot));
}

#[test]
fn aborting_speculation_kills_deferreds_recursively() {
    let mut p = printer(79);
    p.begin_speculative(M).unwrap();
    let outer = p.deferred(M).unwrap();
    let inner = p.deferred(Target::Slot(outer)).unwrap();
    p.begin_speculative(Target::Slot(outer)).unwrap();
    let speculative_child = p.deferred(Target::Slot(outer)).unwrap();
    p.abort_speculative(M).unwrap();
    assert!(!p.slot_is_live(outer));
    assert!(!p.slot_is_live(inner));
    assert!(!p.slot_is_live(speculative_child));
}

#[test]
fn committing_speculation_containing_a_splice_starts_recording() {
    let mut p = printer(79);
    p.begin_speculative(M).unwrap();
    let slot = p.deferred(M).unwrap();
    p.text(M, "y").unwrap();
    p.commit_speculative(M).unwrap();
    p.text(Target::Slot(slot), "x").unwrap();
    p.resolve().unwrap();
    assert_eq!(p.value().unwrap(), "xy");
}

#[test]
fn commit_or_abort_without_begin_is_an_error() {
    let mut p = printer(79);
    assert_eq!(p.commit_speculative(M), Err(PrinterError::NoSpeculation));
    assert_eq!(p.abort_speculative(M), Err(PrinterError::NoSpeculation));
    let slot = p.deferred(M).unwrap();
    assert_eq!(
        p.commit_speculative(Target::Slot(slot)),
        Err(PrinterError::NoSpeculation)
    );
    assert_eq!(
        p.abort_speculative(Target::Slot(slot)),
        Err(PrinterError::NoSpeculation)
    );
}

#[test]
fn resolve_surfaces_unbalanced_groups_after_the_hole() {
    let mut p = printer(79);
    p.deferred(M).unwrap();
    p.end_group(M, "").unwrap();
    assert_eq!(p.resolve(), Err(PrinterError::UnbalancedGroup));
}

#[test]
fn resolve_surfaces_unbalanced_groups_in_slot_content() {
    let mut p = printer(79);
    let slot = p.deferred(M).unwrap();
    p.end_group(Target::Slot(slot), "").unwrap();
    assert_eq!(p.resolve(), Err(PrinterError::UnbalancedGroup));
    assert_eq!(p.value(), Err(PrinterError::UnbalancedGroup));
}

#[test]
fn slot_content_may_close_groups_opened_before_the_hole() {
    let mut p = printer(79);
    p.begin_group(M, 1, "[").unwrap();
    p.text(M, "1").unwrap();
    let slot = p.deferred(M).unwrap();
    p.end_group(Target::Slot(slot), "]").unwrap();
    p.resolve().unwrap();
    assert_eq!(p.value().unwrap(), "[1]");
}

#[test]
fn commit_of_unbalanced_speculation_into_main_errors_atomically() {
    let mut p = printer(79);
    p.text(M, "x").unwrap();
    p.begin_speculative(M).unwrap();
    p.text(M, "junk").unwrap();
    p.end_group(M, ")").unwrap();
    assert_eq!(p.commit_speculative(M), Err(PrinterError::UnbalancedGroup));
    p.abort_speculative(M).unwrap();
    assert_eq!(p.value().unwrap(), "x");
}

#[test]
fn speculation_may_close_groups_opened_outside_it() {
    let mut p = printer(79);
    p.begin_group(M, 0, "(").unwrap();
    p.begin_speculative(M).unwrap();
    p.text(M, "x").unwrap();
    p.end_group(M, ")").unwrap();
    p.commit_speculative(M).unwrap();
    assert_eq!(p.value().unwrap(), "(x)");
}

#[test]
fn nested_speculation_commits_are_not_validated_until_the_outer_commit() {
    let mut p = printer(79);
    p.begin_speculative(M).unwrap();
    p.begin_speculative(M).unwrap();
    p.end_group(M, ")").unwrap();
    p.commit_speculative(M).unwrap();
    assert_eq!(p.commit_speculative(M), Err(PrinterError::UnbalancedGroup));
    p.abort_speculative(M).unwrap();
    assert_eq!(p.value().unwrap(), "");
}

#[test]
fn value_can_be_read_repeatedly_and_interleaved_with_writes() {
    let mut p = printer(79);
    p.text(M, "a").unwrap();
    assert_eq!(p.value().unwrap(), "a");
    assert_eq!(p.value().unwrap(), "a");
    p.text(M, "b").unwrap();
    assert_eq!(p.value().unwrap(), "ab");
}

// gofmt-style layout: opening brace followed by a breakable, a trailing
// if_break comma, and a breakable before the close, so the broken form puts
// every element on its own line with a trailing comma and the close brace at
// the outer indentation, while the flat form stays `{1, 2, 3}`.
fn gofmt_list(p: &mut Printer, elements: &[&str]) {
    p.begin_group(M, 0, "{").unwrap();
    p.shift_indent(M, 4).unwrap();
    for (i, e) in elements.iter().enumerate() {
        if i == 0 {
            p.breakable(M, "").unwrap();
        } else {
            p.text(M, ",").unwrap();
            p.breakable(M, " ").unwrap();
        }
        p.text(M, e).unwrap();
    }
    p.shift_indent(M, -4).unwrap();
    if !elements.is_empty() {
        p.if_break(M, ",").unwrap();
        p.breakable(M, "").unwrap();
    }
    p.end_group(M, "}").unwrap();
}

#[test]
fn if_break_renders_nothing_in_a_fitting_group() {
    let mut p = printer(79);
    gofmt_list(&mut p, &["1", "2", "3"]);
    assert_eq!(p.value().unwrap(), "{1, 2, 3}");
}

#[test]
fn if_break_emits_its_text_when_the_group_breaks_by_width() {
    let mut p = printer(6);
    gofmt_list(&mut p, &["1", "2", "3"]);
    assert_eq!(p.value().unwrap(), "{\n    1,\n    2,\n    3,\n}");
}

#[test]
fn if_break_emits_immediately_in_an_already_broken_group() {
    let mut p = printer(4);
    p.begin_group(M, 0, "{").unwrap();
    p.text(M, "aaaaaa").unwrap();
    p.breakable(M, " ").unwrap();
    p.text(M, "b").unwrap();
    p.if_break(M, ",").unwrap();
    p.breakable(M, "").unwrap();
    p.end_group(M, "}").unwrap();
    assert_eq!(p.value().unwrap(), "{aaaaaa\nb,\n}");
}

#[test]
fn if_break_does_not_count_toward_width() {
    let mut p = printer(9);
    gofmt_list(&mut p, &["1", "2", "3"]);
    assert_eq!(p.value().unwrap(), "{1, 2, 3}");
}

#[test]
fn if_break_in_a_comment_forced_group_emits_before_the_close_break() {
    let mut p = printer(79);
    p.begin_group(M, 0, "{").unwrap();
    p.shift_indent(M, 4).unwrap();
    p.breakable(M, "").unwrap();
    p.text(M, "1").unwrap();
    p.comment(M, " // c").unwrap();
    p.text(M, ",").unwrap();
    p.breakable(M, " ").unwrap();
    p.text(M, "2").unwrap();
    p.shift_indent(M, -4).unwrap();
    p.if_break(M, ",").unwrap();
    p.breakable(M, "").unwrap();
    p.end_group(M, "}").unwrap();
    assert_eq!(p.value().unwrap(), "{\n    1, // c\n    2,\n}");
}

#[test]
fn comment_forced_close_break_is_not_doubled_after_an_explicit_breakable() {
    let mut p = printer(79);
    p.begin_group(M, 0, "{").unwrap();
    p.shift_indent(M, 4).unwrap();
    p.breakable(M, "").unwrap();
    p.text(M, "1").unwrap();
    p.comment(M, " // c").unwrap();
    p.shift_indent(M, -4).unwrap();
    p.if_break(M, ",").unwrap();
    p.breakable(M, "").unwrap();
    p.end_group(M, "}").unwrap();
    assert_eq!(p.value().unwrap(), "{\n    1, // c\n}");
}

#[test]
fn comment_is_emitted_at_the_end_of_its_line() {
    let mut p = printer(79);
    p.text(M, "a").unwrap();
    p.comment(M, "  // c").unwrap();
    p.text(M, "b").unwrap();
    p.hard_break(M).unwrap();
    p.text(M, "d").unwrap();
    assert_eq!(p.value().unwrap(), "ab  // c\nd");
}

#[test]
fn comment_at_end_of_document_appears_without_a_newline() {
    let mut p = printer(79);
    p.text(M, "x").unwrap();
    p.comment(M, "  // c").unwrap();
    assert_eq!(p.value().unwrap(), "x  // c");
}

#[test]
fn comment_forces_every_open_group_to_break() {
    let mut p = printer(79);
    p.begin_group(M, 1, "[").unwrap();
    p.text(M, "1").unwrap();
    p.text(M, ",").unwrap();
    p.breakable(M, " ").unwrap();
    p.text(M, "2").unwrap();
    p.comment(M, "  // or any other generated value").unwrap();
    p.text(M, ",").unwrap();
    p.breakable(M, " ").unwrap();
    p.text(M, "3").unwrap();
    p.end_group(M, "]").unwrap();
    assert_eq!(
        p.value().unwrap(),
        "[1,\n 2,  // or any other generated value\n 3\n]"
    );
}

#[test]
fn comment_breaks_earlier_breakables_of_its_groups() {
    let mut p = printer(79);
    p.begin_group(M, 1, "[").unwrap();
    p.text(M, "1,").unwrap();
    p.breakable(M, " ").unwrap();
    p.text(M, "2,").unwrap();
    p.breakable(M, " ").unwrap();
    p.text(M, "3").unwrap();
    p.comment(M, "  // c").unwrap();
    p.end_group(M, "]").unwrap();
    assert_eq!(p.value().unwrap(), "[1,\n 2,\n 3  // c\n]");
}

#[test]
fn comment_does_not_break_groups_opened_after_it() {
    let mut p = printer(79);
    p.begin_group(M, 1, "[").unwrap();
    p.text(M, "1").unwrap();
    p.comment(M, "  // c").unwrap();
    p.text(M, ",").unwrap();
    p.breakable(M, " ").unwrap();
    p.begin_group(M, 1, "(").unwrap();
    p.text(M, "2,").unwrap();
    p.breakable(M, " ").unwrap();
    p.text(M, "3").unwrap();
    p.end_group(M, ")").unwrap();
    p.end_group(M, "]").unwrap();
    assert_eq!(p.value().unwrap(), "[1,  // c\n (2, 3)\n]");
}

#[test]
fn nested_comment_forced_groups_each_break_before_their_close() {
    let mut p = printer(79);
    p.begin_group(M, 1, "(").unwrap();
    p.begin_group(M, 1, "[").unwrap();
    p.text(M, "1").unwrap();
    p.comment(M, "  // c").unwrap();
    p.end_group(M, "]").unwrap();
    p.text(M, ",").unwrap();
    p.end_group(M, ")").unwrap();
    assert_eq!(p.value().unwrap(), "([1  // c\n ],\n)");
}

#[test]
fn comment_break_trims_leading_whitespace_from_the_close_text() {
    let mut p = printer(79);
    p.begin_group(M, 4, "S {").unwrap();
    p.breakable(M, " ").unwrap();
    p.text(M, "a: 1").unwrap();
    p.comment(M, "  // c").unwrap();
    p.end_group(M, " }").unwrap();
    assert_eq!(p.value().unwrap(), "S {\n    a: 1  // c\n}");
}

#[test]
fn comment_outside_any_group_does_not_poison_later_breakables() {
    let mut p = printer(20);
    p.text(M, "a").unwrap();
    p.comment(M, "  // c").unwrap();
    p.hard_break(M).unwrap();
    p.text(M, "1").unwrap();
    p.breakable(M, " ").unwrap();
    p.text(M, "2").unwrap();
    assert_eq!(p.value().unwrap(), "a  // c\n1 2");
}

#[test]
fn comment_does_not_count_toward_width() {
    let mut p = printer(10);
    p.text(M, "12").unwrap();
    p.comment(M, "  // aaaaaaaaaaaaaaaaaaaaaaaaaa").unwrap();
    p.breakable(M, " ").unwrap();
    p.text(M, "45").unwrap();
    assert_eq!(p.value().unwrap(), "12 45  // aaaaaaaaaaaaaaaaaaaaaaaaaa");
}

#[test]
fn comment_before_a_width_forced_break_stays_on_its_own_line() {
    let mut p = printer(2);
    p.text(M, "12").unwrap();
    p.comment(M, " // c").unwrap();
    p.breakable(M, " ").unwrap();
    p.text(M, "45").unwrap();
    assert_eq!(p.value().unwrap(), "12 // c\n45");
}

#[test]
fn comment_after_a_buffered_breakable_attaches_to_the_later_line() {
    let mut p = printer(4);
    p.text(M, "ab").unwrap();
    p.breakable(M, " ").unwrap();
    p.text(M, "cd").unwrap();
    p.comment(M, " // c").unwrap();
    p.text(M, "ef").unwrap();
    assert_eq!(p.value().unwrap(), "ab\ncdef // c");
}

#[test]
fn multiple_comments_on_one_line_concatenate_in_order() {
    let mut p = printer(79);
    p.text(M, "a").unwrap();
    p.comment(M, "  // one").unwrap();
    p.comment(M, "  // two").unwrap();
    p.hard_break(M).unwrap();
    assert_eq!(p.value().unwrap(), "a  // one  // two\n");
}

#[test]
fn comment_in_an_aborted_speculation_is_dropped() {
    let mut p = printer(79);
    p.begin_group(M, 1, "[").unwrap();
    p.text(M, "1").unwrap();
    p.begin_speculative(M).unwrap();
    p.comment(M, "  // c").unwrap();
    p.abort_speculative(M).unwrap();
    p.text(M, ",").unwrap();
    p.breakable(M, " ").unwrap();
    p.text(M, "2").unwrap();
    p.end_group(M, "]").unwrap();
    assert_eq!(p.value().unwrap(), "[1, 2]");
}

#[test]
fn comment_in_a_committed_speculation_takes_effect() {
    let mut p = printer(79);
    p.begin_group(M, 1, "[").unwrap();
    p.text(M, "1").unwrap();
    p.begin_speculative(M).unwrap();
    p.comment(M, "  // c").unwrap();
    p.commit_speculative(M).unwrap();
    p.text(M, ",").unwrap();
    p.breakable(M, " ").unwrap();
    p.text(M, "2").unwrap();
    p.end_group(M, "]").unwrap();
    assert_eq!(p.value().unwrap(), "[1,  // c\n 2\n]");
}

#[test]
fn comment_inside_a_deferred_slot_breaks_the_groups_open_at_the_hole() {
    let mut p = printer(79);
    p.begin_group(M, 1, "[").unwrap();
    p.text(M, "1,").unwrap();
    p.breakable(M, " ").unwrap();
    let slot = p.deferred(M).unwrap();
    p.end_group(M, "]").unwrap();
    p.text(Target::Slot(slot), "2").unwrap();
    p.comment(Target::Slot(slot), "  // c").unwrap();
    p.resolve().unwrap();
    assert_eq!(p.value().unwrap(), "[1,\n 2  // c\n]");
}

#[test]
fn comment_on_a_dead_slot_is_an_error() {
    let mut p = printer(79);
    let slot = p.deferred(M).unwrap();
    p.resolve().unwrap();
    assert_eq!(
        p.comment(Target::Slot(slot), "  // c"),
        Err(PrinterError::DeadSlot)
    );
}

#[test]
fn errors_have_readable_messages() {
    assert_eq!(
        PrinterError::DeadSlot.to_string(),
        "deferred slot used after its printing session ended"
    );
    assert_eq!(
        PrinterError::UnbalancedGroup.to_string(),
        "end_group without a matching begin_group"
    );
    assert_eq!(
        PrinterError::NoSpeculation.to_string(),
        "commit or abort without an open speculative region"
    );
    assert_eq!(
        PrinterError::OpenSpeculation.to_string(),
        "operation requires all speculative regions to be closed"
    );
    assert_eq!(
        PrinterError::UnresolvedDeferred.to_string(),
        "printer has unresolved deferred slots"
    );
    assert_eq!(
        PrinterError::NothingToResolve.to_string(),
        "resolve called with no outstanding deferred slots"
    );
}

#[test]
fn note_emits_whole_lines() {
    let mut p = printer(79);
    p.note("hello");
    assert_eq!(p.value().unwrap(), "hello\n");
}

#[test]
fn note_splits_embedded_newlines_into_hard_breaks() {
    let mut p = printer(5);
    p.note("aaaaaaaa\nbb");
    p.note("");
    assert_eq!(p.value().unwrap(), "aaaaaaaa\nbb\n\n");
}

#[test]
fn note_respects_indentation_and_recording() {
    let mut p = printer(79);
    p.shift_indent(M, 2).unwrap();
    let slot = p.deferred(M).unwrap();
    p.note("after");
    p.text(Target::Slot(slot), "x").unwrap();
    p.resolve().unwrap();
    assert_eq!(p.value().unwrap(), "xafter\n  ");
}

#[derive(Debug, Clone)]
enum Op {
    Text(String),
    Breakable(String),
    HardBreak,
    BeginGroup { indent: usize, open: String },
    EndGroup { close: String },
    ShiftIndent(isize),
    Comment(String),
}

fn apply(p: &mut Printer, target: Target, op: &Op) {
    match op {
        Op::Text(s) => p.text(target, s).unwrap(),
        Op::Breakable(sep) => p.breakable(target, sep).unwrap(),
        Op::HardBreak => p.hard_break(target).unwrap(),
        Op::BeginGroup { indent, open } => p.begin_group(target, *indent, open).unwrap(),
        Op::EndGroup { close } => p.end_group(target, close).unwrap(),
        Op::ShiftIndent(delta) => p.shift_indent(target, *delta).unwrap(),
        Op::Comment(s) => p.comment(target, s).unwrap(),
    }
}

fn random_text(rng: &mut SmallRng) -> String {
    let alphabet = ['a', 'b', 'x', 'é'];
    let len = rng.random_range(1..6);
    (0..len)
        .map(|_| alphabet[rng.random_range(0..alphabet.len())])
        .collect()
}

fn random_program(rng: &mut SmallRng) -> Vec<Op> {
    let mut ops = Vec::new();
    let mut open = 0usize;
    let mut comments = 0usize;
    for _ in 0..rng.random_range(1..60) {
        ops.push(match rng.random_range(0..9) {
            0..=2 => Op::Text(random_text(rng)),
            3..=4 => Op::Breakable([" ", ", ", ""][rng.random_range(0..3)].to_string()),
            5 => Op::HardBreak,
            6 => {
                if open > 0 && rng.random_bool(0.5) {
                    open -= 1;
                    Op::EndGroup {
                        close: ["", "]", ")"][rng.random_range(0..3)].to_string(),
                    }
                } else {
                    open += 1;
                    Op::BeginGroup {
                        indent: rng.random_range(0..3),
                        open: ["", "[", "("][rng.random_range(0..3)].to_string(),
                    }
                }
            }
            7 => Op::ShiftIndent(rng.random_range(-2i64..4) as isize),
            _ => {
                comments += 1;
                Op::Comment(format!(" ©{comments}"))
            }
        });
    }
    for _ in 0..open {
        ops.push(Op::EndGroup {
            close: String::new(),
        });
    }
    ops
}

fn maybe_insert_aborted_junk(p: &mut Printer, rng: &mut SmallRng) {
    if !rng.random_bool(0.1) {
        return;
    }
    p.begin_speculative(M).unwrap();
    for _ in 0..rng.random_range(1..4) {
        match rng.random_range(0..5) {
            0 => p.text(M, &random_text(rng)).unwrap(),
            1 => p.breakable(M, " ").unwrap(),
            2 => p.hard_break(M).unwrap(),
            3 => p.comment(M, " ©junk").unwrap(),
            _ => {
                p.deferred(M).unwrap();
            }
        }
    }
    p.abort_speculative(M).unwrap();
}

#[test]
fn deferred_and_speculative_printing_matches_inline() {
    for seed in 0..200u64 {
        let mut rng = SmallRng::seed_from_u64(seed);
        let max_width = rng.random_range(1..40);
        let ops = random_program(&mut rng);

        let mut model = printer(max_width);
        for op in &ops {
            apply(&mut model, M, op);
        }

        let mut subject = printer(max_width);
        let mut chunks: Vec<(SlotId, Vec<Op>)> = Vec::new();
        let mut i = 0;
        while i < ops.len() {
            maybe_insert_aborted_junk(&mut subject, &mut rng);
            let roll = rng.random_range(0..10);
            if roll < 2 {
                let len = rng.random_range(1..=(ops.len() - i).min(5));
                let slot = subject.deferred(M).unwrap();
                chunks.push((slot, ops[i..i + len].to_vec()));
                i += len;
            } else if roll < 4 {
                let len = rng.random_range(1..=(ops.len() - i).min(5));
                subject.begin_speculative(M).unwrap();
                for op in &ops[i..i + len] {
                    apply(&mut subject, M, op);
                }
                subject.commit_speculative(M).unwrap();
                i += len;
            } else {
                apply(&mut subject, M, &ops[i]);
                i += 1;
            }
        }
        chunks.shuffle(&mut rng);
        for (slot, chunk) in &chunks {
            for op in chunk {
                apply(&mut subject, Target::Slot(*slot), op);
            }
        }
        if !chunks.is_empty() {
            subject.resolve().unwrap();
        }

        assert_eq!(
            subject.value().unwrap(),
            model.value().unwrap(),
            "seed {seed}"
        );
    }
}

#[test]
fn comments_always_terminate_their_line_and_appear_exactly_once() {
    for seed in 0..200u64 {
        let mut rng = SmallRng::seed_from_u64(seed + 1000);
        let max_width = rng.random_range(1..40);
        let ops = random_program(&mut rng);
        let comments = ops.iter().filter(|op| matches!(op, Op::Comment(_))).count();

        let mut p = printer(max_width);
        for op in &ops {
            apply(&mut p, M, op);
        }
        let output = p.value().unwrap().to_string();

        assert_eq!(
            output.matches('©').count(),
            comments,
            "seed {seed}: every comment should appear exactly once\n{output}"
        );
        for line in output.lines() {
            if let Some(position) = line.find('©') {
                assert!(
                    line[position..]
                        .chars()
                        .all(|c| c == '©' || c == ' ' || c.is_ascii_digit()),
                    "seed {seed}: comment must be the last thing on its line: {line:?}"
                );
            }
        }
    }
}
