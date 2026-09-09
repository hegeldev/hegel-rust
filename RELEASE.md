RELEASE_TYPE: patch

This patch improves the diagnostic for a flaky test. When a shrunk failure no longer fails on its final replay, the `Flaky test detected` message now also names the failure that did not reproduce, as the panic location the engine recorded for it.

`PrettyPrinter::should_print` now also reports `false` for a printer whose region has died — a clone that outlived the document it was printing into — since its writes are discarded.

Internally, `TestCase::note` now goes through the engine's own note primitive instead of assembling note lines from lower-level printing calls.
