RELEASE_TYPE: patch

This patch adds two functions for shaping a test case's printed output without decorating every line by hand.

`hegel_test_case_block` opens a handle onto the same choice stream as an existing handle whose print region is a block nested in the parent's: every line printed or noted through it — and through the clones and blocks derived from it — is indented a given number of columns further than the parent's lines, and the indentation ends exactly with the block. This is how a binding prints the body of a stateful rule under its `Step 3: add {` heading.

`hegel_test_case_set_worker` attributes a handle's output to a concurrent worker: every line recorded through it from then on, notes and printer lines alike, is prefixed with `[worker N +X.XXXms] `, stamped with the time since the test case started at which the line was recorded. Blocks and clones derived from the handle inherit the attribution.

To make block indentation possible, a line's indentation is now written when the line gets its first content rather than at the newline that started it. Documents without blocks render exactly as before, including the padding of blank lines and of a trailing hard break.

This patch also changes when `hegel_note` appends its text. A note appended while a speculative region is open on the handle's print region — the client is mid-way through printing a drawn value, and the note comes from inside that value's generation — is now held back and appended once the outermost region closes, whether it is committed or aborted. Previously the note's lines were spliced into the value being printed. Notes appended outside a speculative region are unaffected.
