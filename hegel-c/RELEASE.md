RELEASE_TYPE: patch

This patch adds a pretty-printing document API to the C ABI, so frontends
can report drawn values through the engine instead of formatting them
by hand.

The engine ships an Oppen-style layout core as a set of building blocks:
`hegel_printer_new` creates a standalone document (configured through a
`hegel_printer_options_t` handle), and `hegel_printer_text`,
`hegel_printer_breakable`, `hegel_printer_begin_group` /
`hegel_printer_end_group`, `hegel_printer_hard_break`,
`hegel_printer_if_break`, `hegel_printer_shift_indent`, and
`hegel_printer_comment` build its content. What gets printed — and in which
language's syntax — is entirely the frontend's choice; the engine only owns
the layout. Two facilities support printing values while generating them:
`hegel_printer_deferred` opens a hole whose content is written later and
spliced in on read, and `hegel_printer_begin_speculative` /
`hegel_printer_commit_speculative` / `hegel_printer_abort_speculative`
buffer output that a rejected draw (a filter retry, a duplicate collection
element) can retract. Reading a document with `hegel_printer_resolve` /
`hegel_printer_value` seals it: open speculative regions are aborted and
later writes on any handle become dead-region errors, so a straggling
writer thread cannot corrupt a rendered report.

Every test case owns one such document, shared by all handles of its
family: `hegel_test_case_printer` fetches a handle onto it and `hegel_note`
appends free-form note lines. Each test-case handle writes into its own
region of the document, and `hegel_test_case_clone` anchors the clone's
region at the point in the parent's output where the clone was made, so
concurrent generation from cloned handles renders deterministically no
matter how the threads interleave.

Frontends that do not use the document API are unaffected.
