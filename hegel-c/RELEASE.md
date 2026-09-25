RELEASE_TYPE: patch

This patch updates the `dashu-int` dependency from 0.4.1 to 0.6.1, picking up correctness fixes and speedups in the arbitrary-precision integer backend behind the shortlex index arithmetic. The `IBig`/`UBig` API surface the engine uses is unchanged; the dependency also drops its `rustversion` build-time dependency.

This patch also improves the performance of text generators that are constructed inside the test body, which is how most tests write them. Building a text generator's alphabet from its codec, codepoint bounds, Unicode categories and included or excluded characters cost more than the draws made from it, and was repeated for every test case; the engine now shares the built alphabet between generators with the same constraints, so a test drawing one short string now runs in about half the instructions per test case.
