RELEASE_TYPE: patch

A test that rejects its input via `assume()` without drawing any data can never produce a valid case. The engine now stops after one call and fails the run with `Unsatisfiable`, instead of passing. Over the C ABI this surfaces as an ordinary error result; no signatures or status values change.
