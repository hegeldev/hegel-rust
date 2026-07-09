//! In-process exercise of the `hegel_printer_*` / `hegel_note` C ABI: the
//! layout primitives, deferred slots, speculative regions, the per-family
//! document, and every argument-validation path.

mod common;

use common::{last_error, make_settings_no_db, next_case, ok, start};
use hegel_c::hegel_result_t::*;
use hegel_c::{
    HegelContext, HegelPrinter, hegel_context_free, hegel_context_new, hegel_mark_complete,
    hegel_note, hegel_printer_abort_speculative, hegel_printer_begin_group,
    hegel_printer_begin_speculative, hegel_printer_breakable, hegel_printer_comment,
    hegel_printer_commit_speculative, hegel_printer_deferred, hegel_printer_end_group,
    hegel_printer_free, hegel_printer_hard_break, hegel_printer_if_break, hegel_printer_is_live,
    hegel_printer_new, hegel_printer_resolve, hegel_printer_shift_indent, hegel_printer_text,
    hegel_printer_value, hegel_printer_value_result_free, hegel_printer_value_result_t,
    hegel_run_free, hegel_settings_free, hegel_status_t, hegel_test_case_clone,
    hegel_test_case_free, hegel_test_case_printer,
};
use std::ptr;

unsafe fn new_printer(ctx: *mut HegelContext, max_width: u64) -> *mut HegelPrinter {
    let mut p: *mut HegelPrinter = ptr::null_mut();
    ok(unsafe { hegel_printer_new(ctx, max_width, &mut p) });
    assert!(!p.is_null());
    p
}

unsafe fn text(ctx: *mut HegelContext, p: *mut HegelPrinter, s: &str) {
    ok(unsafe { hegel_printer_text(ctx, p, s.as_ptr(), s.len()) });
}

unsafe fn value(ctx: *mut HegelContext, p: *mut HegelPrinter) -> String {
    let mut result = hegel_printer_value_result_t {
        data: ptr::null_mut(),
        len: 0,
    };
    ok(unsafe { hegel_printer_value(ctx, p, &mut result) });
    assert!(!result.data.is_null());
    let bytes =
        unsafe { std::slice::from_raw_parts(result.data.cast::<u8>(), result.len) }.to_vec();
    ok(unsafe { hegel_printer_value_result_free(ctx, &mut result) });
    assert!(result.data.is_null());
    assert_eq!(result.len, 0);
    String::from_utf8(bytes).unwrap()
}

#[test]
fn groups_lay_out_inline_or_broken() {
    let ctx = hegel_context_new();
    unsafe {
        let p = new_printer(ctx, 20);
        ok(hegel_printer_begin_group(ctx, p, 1, "[".as_ptr(), 1));
        text(ctx, p, "1,");
        ok(hegel_printer_breakable(ctx, p, " ".as_ptr(), 1));
        text(ctx, p, "2");
        ok(hegel_printer_end_group(ctx, p, 1, "]".as_ptr(), 1));
        assert_eq!(value(ctx, p), "[1, 2]");
        ok(hegel_printer_free(ctx, p));

        let p = new_printer(ctx, 6);
        ok(hegel_printer_begin_group(ctx, p, 1, "[".as_ptr(), 1));
        text(ctx, p, "1,");
        ok(hegel_printer_breakable(ctx, p, " ".as_ptr(), 1));
        text(ctx, p, "2,");
        ok(hegel_printer_breakable(ctx, p, " ".as_ptr(), 1));
        text(ctx, p, "3");
        ok(hegel_printer_end_group(ctx, p, 1, "]".as_ptr(), 1));
        assert_eq!(value(ctx, p), "[1,\n 2,\n 3]");
        ok(hegel_printer_free(ctx, p));
        ok(hegel_context_free(ctx));
    }
}

#[test]
fn shift_indent_applies_to_hard_breaks_and_clamps() {
    let ctx = hegel_context_new();
    unsafe {
        let p = new_printer(ctx, 79);
        text(ctx, p, "a");
        ok(hegel_printer_shift_indent(ctx, p, 4));
        ok(hegel_printer_hard_break(ctx, p));
        text(ctx, p, "b");
        ok(hegel_printer_shift_indent(ctx, p, i64::MIN));
        ok(hegel_printer_hard_break(ctx, p));
        text(ctx, p, "c");
        assert_eq!(value(ctx, p), "a\n    b\nc");
        ok(hegel_printer_free(ctx, p));
        ok(hegel_context_free(ctx));
    }
}

#[test]
fn deferred_slot_roundtrip_and_death() {
    let ctx = hegel_context_new();
    unsafe {
        let p = new_printer(ctx, 79);
        text(ctx, p, "a");
        let mut slot: *mut HegelPrinter = ptr::null_mut();
        ok(hegel_printer_deferred(ctx, p, &mut slot));
        assert!(!slot.is_null());
        text(ctx, p, "c");

        let mut live = false;
        ok(hegel_printer_is_live(ctx, p, &mut live));
        assert!(live);
        ok(hegel_printer_is_live(ctx, slot, &mut live));
        assert!(live);

        let mut result = hegel_printer_value_result_t {
            data: ptr::null_mut(),
            len: 0,
        };
        assert_eq!(
            hegel_printer_value(ctx, p, &mut result),
            HEGEL_E_INVALID_ARG
        );
        assert!(last_error(ctx).contains("unresolved deferred"));

        text(ctx, slot, "b");
        ok(hegel_printer_resolve(ctx, p));
        assert_eq!(value(ctx, p), "abc");

        ok(hegel_printer_is_live(ctx, slot, &mut live));
        assert!(!live);
        assert_eq!(
            hegel_printer_text(ctx, slot, "x".as_ptr(), 1),
            HEGEL_E_INVALID_ARG
        );
        assert!(last_error(ctx).contains("session ended"));
        assert_eq!(
            hegel_printer_breakable(ctx, slot, " ".as_ptr(), 1),
            HEGEL_E_INVALID_ARG
        );
        assert_eq!(
            hegel_printer_if_break(ctx, slot, ",".as_ptr(), 1),
            HEGEL_E_INVALID_ARG
        );
        assert_eq!(hegel_printer_hard_break(ctx, slot), HEGEL_E_INVALID_ARG);
        assert_eq!(
            hegel_printer_begin_group(ctx, slot, 0, ptr::null(), 0),
            HEGEL_E_INVALID_ARG
        );
        assert_eq!(
            hegel_printer_end_group(ctx, slot, 0, ptr::null(), 0),
            HEGEL_E_INVALID_ARG
        );
        assert_eq!(
            hegel_printer_shift_indent(ctx, slot, 1),
            HEGEL_E_INVALID_ARG
        );
        let mut grandchild: *mut HegelPrinter = ptr::null_mut();
        assert_eq!(
            hegel_printer_deferred(ctx, slot, &mut grandchild),
            HEGEL_E_INVALID_ARG
        );
        assert_eq!(
            hegel_printer_begin_speculative(ctx, slot),
            HEGEL_E_INVALID_ARG
        );
        assert_eq!(
            hegel_printer_commit_speculative(ctx, slot),
            HEGEL_E_INVALID_ARG
        );
        assert_eq!(
            hegel_printer_abort_speculative(ctx, slot),
            HEGEL_E_INVALID_ARG
        );

        ok(hegel_printer_free(ctx, slot));
        ok(hegel_printer_free(ctx, p));
        ok(hegel_context_free(ctx));
    }
}

#[test]
fn resolve_and_value_reject_slot_handles() {
    let ctx = hegel_context_new();
    unsafe {
        let p = new_printer(ctx, 79);
        let mut slot: *mut HegelPrinter = ptr::null_mut();
        ok(hegel_printer_deferred(ctx, p, &mut slot));
        assert_eq!(hegel_printer_resolve(ctx, slot), HEGEL_E_INVALID_ARG);
        assert!(last_error(ctx).contains("root handle"));
        let mut result = hegel_printer_value_result_t {
            data: ptr::null_mut(),
            len: 0,
        };
        assert_eq!(
            hegel_printer_value(ctx, slot, &mut result),
            HEGEL_E_INVALID_ARG
        );
        assert!(last_error(ctx).contains("root handle"));
        ok(hegel_printer_resolve(ctx, p));
        assert_eq!(hegel_printer_resolve(ctx, p), HEGEL_E_INVALID_ARG);
        assert!(last_error(ctx).contains("no outstanding"));
        ok(hegel_printer_free(ctx, slot));
        ok(hegel_printer_free(ctx, p));
        ok(hegel_context_free(ctx));
    }
}

#[test]
fn speculative_regions_commit_and_abort() {
    let ctx = hegel_context_new();
    unsafe {
        let p = new_printer(ctx, 79);
        text(ctx, p, "a");
        ok(hegel_printer_begin_speculative(ctx, p));
        text(ctx, p, "b");
        ok(hegel_printer_abort_speculative(ctx, p));
        ok(hegel_printer_begin_speculative(ctx, p));
        text(ctx, p, "c");
        ok(hegel_printer_commit_speculative(ctx, p));
        assert_eq!(value(ctx, p), "ac");

        assert_eq!(
            hegel_printer_commit_speculative(ctx, p),
            HEGEL_E_INVALID_ARG
        );
        assert!(last_error(ctx).contains("without an open speculative region"));
        assert_eq!(hegel_printer_abort_speculative(ctx, p), HEGEL_E_INVALID_ARG);
        ok(hegel_printer_free(ctx, p));
        ok(hegel_context_free(ctx));
    }
}

#[test]
fn comments_attach_to_line_ends_and_break_open_groups() {
    let ctx = hegel_context_new();
    unsafe {
        let p = new_printer(ctx, 79);
        ok(hegel_printer_begin_group(ctx, p, 1, "[".as_ptr(), 1));
        text(ctx, p, "1,");
        ok(hegel_printer_breakable(ctx, p, " ".as_ptr(), 1));
        text(ctx, p, "2");
        let comment = "  // or any other generated value";
        ok(hegel_printer_comment(
            ctx,
            p,
            comment.as_ptr(),
            comment.len(),
        ));
        text(ctx, p, ",");
        ok(hegel_printer_breakable(ctx, p, " ".as_ptr(), 1));
        text(ctx, p, "3");
        ok(hegel_printer_end_group(ctx, p, 1, "]".as_ptr(), 1));
        assert_eq!(
            value(ctx, p),
            "[1,\n 2,  // or any other generated value\n 3\n]"
        );
        ok(hegel_printer_free(ctx, p));
        ok(hegel_context_free(ctx));
    }
}

#[test]
fn comment_arguments_are_validated() {
    let ctx = hegel_context_new();
    unsafe {
        let p = new_printer(ctx, 79);
        assert_eq!(
            hegel_printer_comment(ctx, p, "a\nb".as_ptr(), 3),
            HEGEL_E_INVALID_ARG
        );
        assert!(last_error(ctx).contains("must not contain newlines"));
        assert_eq!(
            hegel_printer_comment(ctx, p, ptr::null(), 1),
            HEGEL_E_INVALID_ARG
        );
        assert!(last_error(ctx).contains("text is null"));
        ok(hegel_printer_comment(ctx, p, ptr::null(), 0));

        let mut slot: *mut HegelPrinter = ptr::null_mut();
        ok(hegel_printer_deferred(ctx, p, &mut slot));
        ok(hegel_printer_resolve(ctx, p));
        assert_eq!(
            hegel_printer_comment(ctx, slot, "x".as_ptr(), 1),
            HEGEL_E_INVALID_ARG
        );
        assert!(last_error(ctx).contains("session ended"));

        let null: *mut HegelPrinter = ptr::null_mut();
        assert_eq!(
            hegel_printer_comment(ctx, null, "x".as_ptr(), 1),
            HEGEL_E_INVALID_HANDLE
        );
        ok(hegel_printer_free(ctx, slot));
        ok(hegel_printer_free(ctx, p));
        ok(hegel_context_free(ctx));
    }
}

#[test]
fn text_arguments_are_validated() {
    let ctx = hegel_context_new();
    unsafe {
        let p = new_printer(ctx, 79);
        assert_eq!(
            hegel_printer_text(ctx, p, [0xff_u8].as_ptr(), 1),
            HEGEL_E_INVALID_ARG
        );
        assert!(last_error(ctx).contains("not valid UTF-8"));
        assert_eq!(
            hegel_printer_text(ctx, p, "a\nb".as_ptr(), 3),
            HEGEL_E_INVALID_ARG
        );
        assert!(last_error(ctx).contains("must not contain newlines"));
        assert_eq!(
            hegel_printer_text(ctx, p, ptr::null(), 1),
            HEGEL_E_INVALID_ARG
        );
        assert!(last_error(ctx).contains("text is null"));
        ok(hegel_printer_text(ctx, p, ptr::null(), 0));
        assert_eq!(
            hegel_printer_breakable(ctx, p, "\n".as_ptr(), 1),
            HEGEL_E_INVALID_ARG
        );
        assert_eq!(
            hegel_printer_begin_group(ctx, p, 0, "\n".as_ptr(), 1),
            HEGEL_E_INVALID_ARG
        );
        assert_eq!(
            hegel_printer_end_group(ctx, p, 0, "\n".as_ptr(), 1),
            HEGEL_E_INVALID_ARG
        );
        assert_eq!(
            hegel_printer_end_group(ctx, p, 0, ptr::null(), 0),
            HEGEL_E_INVALID_ARG
        );
        assert!(last_error(ctx).contains("without a matching begin_group"));
        assert_eq!(value(ctx, p), "");
        ok(hegel_printer_free(ctx, p));
        ok(hegel_context_free(ctx));
    }
}

// The gofmt-style layout the if_break primitive exists for: broken form
// with a trailing comma and the close brace on its own line, flat form
// without either.
#[test]
fn if_break_emits_trailing_text_only_in_the_broken_form() {
    let ctx = hegel_context_new();
    unsafe {
        for (max_width, expected) in [(79, "{1, 2}"), (5, "{\n    1,\n    2,\n}")] {
            let p = new_printer(ctx, max_width);
            ok(hegel_printer_begin_group(ctx, p, 0, "{".as_ptr(), 1));
            ok(hegel_printer_shift_indent(ctx, p, 4));
            ok(hegel_printer_breakable(ctx, p, "".as_ptr(), 0));
            text(ctx, p, "1,");
            ok(hegel_printer_breakable(ctx, p, " ".as_ptr(), 1));
            text(ctx, p, "2");
            ok(hegel_printer_shift_indent(ctx, p, -4));
            ok(hegel_printer_if_break(ctx, p, ",".as_ptr(), 1));
            ok(hegel_printer_breakable(ctx, p, "".as_ptr(), 0));
            ok(hegel_printer_end_group(ctx, p, 0, "}".as_ptr(), 1));
            assert_eq!(value(ctx, p), expected);
            ok(hegel_printer_free(ctx, p));
        }
        ok(hegel_context_free(ctx));
    }
}

#[test]
fn if_break_rejects_bad_text() {
    let ctx = hegel_context_new();
    unsafe {
        let p = new_printer(ctx, 79);
        assert_eq!(
            hegel_printer_if_break(ctx, p, [0xff_u8].as_ptr(), 1),
            HEGEL_E_INVALID_ARG
        );
        assert!(last_error(ctx).contains("hegel_printer_if_break"));
        ok(hegel_printer_free(ctx, p));
        ok(hegel_context_free(ctx));
    }
}

#[test]
fn null_handles_and_out_parameters_are_rejected() {
    let ctx = hegel_context_new();
    unsafe {
        assert_eq!(
            hegel_printer_new(ctx, 79, ptr::null_mut()),
            HEGEL_E_INVALID_ARG
        );
        let null: *mut HegelPrinter = ptr::null_mut();
        assert_eq!(
            hegel_printer_text(ctx, null, "a".as_ptr(), 1),
            HEGEL_E_INVALID_HANDLE
        );
        assert_eq!(
            hegel_printer_breakable(ctx, null, " ".as_ptr(), 1),
            HEGEL_E_INVALID_HANDLE
        );
        assert_eq!(
            hegel_printer_if_break(ctx, null, ",".as_ptr(), 1),
            HEGEL_E_INVALID_HANDLE
        );
        assert_eq!(hegel_printer_hard_break(ctx, null), HEGEL_E_INVALID_HANDLE);
        assert_eq!(
            hegel_printer_begin_group(ctx, null, 0, ptr::null(), 0),
            HEGEL_E_INVALID_HANDLE
        );
        assert_eq!(
            hegel_printer_end_group(ctx, null, 0, ptr::null(), 0),
            HEGEL_E_INVALID_HANDLE
        );
        assert_eq!(
            hegel_printer_shift_indent(ctx, null, 1),
            HEGEL_E_INVALID_HANDLE
        );
        let mut slot: *mut HegelPrinter = ptr::null_mut();
        assert_eq!(
            hegel_printer_deferred(ctx, null, &mut slot),
            HEGEL_E_INVALID_HANDLE
        );
        assert_eq!(
            hegel_printer_begin_speculative(ctx, null),
            HEGEL_E_INVALID_HANDLE
        );
        assert_eq!(
            hegel_printer_commit_speculative(ctx, null),
            HEGEL_E_INVALID_HANDLE
        );
        assert_eq!(
            hegel_printer_abort_speculative(ctx, null),
            HEGEL_E_INVALID_HANDLE
        );
        assert_eq!(hegel_printer_resolve(ctx, null), HEGEL_E_INVALID_HANDLE);
        let mut live = false;
        assert_eq!(
            hegel_printer_is_live(ctx, null, &mut live),
            HEGEL_E_INVALID_HANDLE
        );
        let mut result = hegel_printer_value_result_t {
            data: ptr::null_mut(),
            len: 0,
        };
        assert_eq!(
            hegel_printer_value(ctx, null, &mut result),
            HEGEL_E_INVALID_HANDLE
        );

        let p = new_printer(ctx, 79);
        assert_eq!(
            hegel_printer_deferred(ctx, p, ptr::null_mut()),
            HEGEL_E_INVALID_ARG
        );
        assert_eq!(
            hegel_printer_is_live(ctx, p, ptr::null_mut()),
            HEGEL_E_INVALID_ARG
        );
        assert_eq!(
            hegel_printer_value(ctx, p, ptr::null_mut()),
            HEGEL_E_INVALID_ARG
        );
        ok(hegel_printer_free(ctx, p));
        ok(hegel_printer_free(ctx, ptr::null_mut()));
        ok(hegel_printer_value_result_free(ctx, ptr::null_mut()));
        ok(hegel_context_free(ctx));
    }
}

#[test]
fn family_document_is_shared_and_survives_completion() {
    let ctx = hegel_context_new();
    unsafe {
        let s = make_settings_no_db(ctx);
        let run = start(ctx, s);
        let tc = next_case(ctx, run);
        assert!(!tc.is_null());

        ok(hegel_note(ctx, tc, "note one\nsecond line".as_ptr(), 20));

        let mut p1: *mut HegelPrinter = ptr::null_mut();
        ok(hegel_test_case_printer(ctx, tc, 40, &mut p1));
        text(ctx, p1, "let x = 1;");
        ok(hegel_printer_hard_break(ctx, p1));

        let mut clone: *mut hegel_c::HegelTestCase = ptr::null_mut();
        ok(hegel_test_case_clone(ctx, tc, &mut clone));
        let mut p2: *mut HegelPrinter = ptr::null_mut();
        ok(hegel_test_case_printer(ctx, clone, 999, &mut p2));
        text(ctx, p2, "from clone");

        ok(hegel_note(ctx, tc, ptr::null(), 0));

        ok(hegel_mark_complete(
            ctx,
            tc,
            hegel_status_t::HEGEL_STATUS_VALID as u32,
            ptr::null(),
        ));

        assert_eq!(
            value(ctx, p2),
            "note one\nsecond line\nlet x = 1;\nfrom clone\n"
        );

        ok(hegel_printer_free(ctx, p1));
        ok(hegel_printer_free(ctx, p2));
        ok(hegel_test_case_free(ctx, clone));
        ok(hegel_test_case_free(ctx, tc));
        ok(hegel_run_free(ctx, run));
        ok(hegel_settings_free(ctx, s));
        ok(hegel_context_free(ctx));
    }
}

#[test]
fn note_and_test_case_printer_validate_arguments() {
    let ctx = hegel_context_new();
    unsafe {
        assert_eq!(
            hegel_note(ctx, ptr::null_mut(), "x".as_ptr(), 1),
            HEGEL_E_INVALID_HANDLE
        );
        let mut p: *mut HegelPrinter = ptr::null_mut();
        assert_eq!(
            hegel_test_case_printer(ctx, ptr::null_mut(), 79, &mut p),
            HEGEL_E_INVALID_HANDLE
        );

        let s = make_settings_no_db(ctx);
        let run = start(ctx, s);
        let tc = next_case(ctx, run);
        assert!(!tc.is_null());
        assert_eq!(
            hegel_test_case_printer(ctx, tc, 79, ptr::null_mut()),
            HEGEL_E_INVALID_ARG
        );
        assert_eq!(
            hegel_note(ctx, tc, [0xff_u8].as_ptr(), 1),
            HEGEL_E_INVALID_ARG
        );
        assert!(last_error(ctx).contains("not valid UTF-8"));
        assert_eq!(hegel_note(ctx, tc, ptr::null(), 1), HEGEL_E_INVALID_ARG);
        assert!(last_error(ctx).contains("text is null"));

        ok(hegel_mark_complete(
            ctx,
            tc,
            hegel_status_t::HEGEL_STATUS_VALID as u32,
            ptr::null(),
        ));
        ok(hegel_test_case_free(ctx, tc));
        ok(hegel_run_free(ctx, run));
        ok(hegel_settings_free(ctx, s));
        ok(hegel_context_free(ctx));
    }
}
