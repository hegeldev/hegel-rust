RELEASE_TYPE: patch

This patch fixes a misleading diagnostic in the stateful runner. When a `#[rule]` failed for a genuine reason (a panic or failed assertion), the runner printed "Rule stopped early due to violated assumption." just before propagating the failure, even though no assumption had been violated. Conversely, a rule that was genuinely skipped via `assume(false)` printed no such note. The note was wired to the wrong branch; it now appears only when a rule is actually skipped by a violated assumption, and genuine failures propagate without it.
