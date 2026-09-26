RELEASE_TYPE: patch

This patch improves the performance of test case generation and shrinking across the board, without changing the test cases any seed produces or how stored failures replay. Tests that draw many values — derived or hand-written composite structs, vectors, maps, recursive generators — run 27–46% faster per test case; tests that generate strings from regular expressions 12–13% faster; state machine tests 8–28% faster; tests that draw a single value 5–10% faster; and shrinking a failure 4–14% faster.
