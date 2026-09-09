RELEASE_TYPE: patch

This patch improves the diagnostic for a flaky test. When a shrunk failure no longer fails on its final replay, the `Flaky test detected` message now also names the failure that did not reproduce, as the panic location the engine recorded for it.

`PrettyPrinter::should_print` now also reports `false` for a printer whose region has died — a clone that outlived the document it was printing into — since its writes are discarded.

In a concurrent state machine's failure report, every line of a multi-line `tc.note()` made from a worker thread now carries the `[worker N +X.XXXms]` attribution; previously only the note's first line did.

Internally, the frontend now leaves the shape of its output to the engine: `TestCase::note` goes through the engine's own note primitive, the indentation of stateful rule bodies and `tc.repeat` iterations is the engine's block regions, and the worker attribution on concurrent workers' lines is stamped by the engine rather than assembled from lower-level printing calls.
