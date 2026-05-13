RELEASE_TYPE: patch

This patch makes Hegel surface every distinct failing test case from a
run, rather than collapsing multi-bug runs down to whichever failure
fired last.

When the run produces multiple distinct failures, Hegel now prints:

```
Hegel found N failing test cases:
<diagnostic for failure 1>
<diagnostic for failure 2>
...
```
