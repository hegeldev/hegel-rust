RELEASE_TYPE: patch

This patch improves the failure reports of stateful tests. A failing `#[invariant]` now ends the report with `Invariant <name> failed:`, and a panicking `#[rule]` body with `Rule <name> failed:`, instead of leaving only the panic's file and line to identify the failing method. The `Initial invariant check.` line is reworded to `Checking invariants on the initial state.` (likewise for the final check) and no longer printed for machines with no invariants. ([#440](https://github.com/hegeldev/hegel-rust/issues/440))
