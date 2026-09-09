RELEASE_TYPE: patch

This patch changes when `hegel_note` appends its text. A note appended while a speculative region is open on the handle's print region — the client is mid-way through printing a drawn value, and the note comes from inside that value's generation — is now held back and appended once the outermost region closes, whether it is committed or aborted. Previously the note's lines were spliced into the value being printed. Notes appended outside a speculative region are unaffected.
