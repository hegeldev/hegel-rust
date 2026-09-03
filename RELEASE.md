RELEASE_TYPE: patch

This patch fixes the ordering of step labels in stateful counterexamples. Each label used to print after the draws its rule made, so reading a failing sequence meant shifting every label back by one. Notes are also no longer deferred while an engine span is open, only while a drawn value is mid-print, so a note can never trail the output of a later draw.

Each step now prints as a block, with the rule's draws and notes scoped to it:

```
Step 1: add {
  let n = 1;
}
```

`#[rule]` and `#[invariant]` bodies now rewrite `tc.draw` calls the way `#[hegel::test]` bodies do, so a rule's draws print under their variable names (`let n = 1;` instead of `let draw_1 = 1;`) and `tc.target` calls get per-expression labels. Draw names are scoped to the rule invocation: a name drawn once per rule prints bare in every step, and only names drawn repeatedly within one invocation get a numeric suffix.
