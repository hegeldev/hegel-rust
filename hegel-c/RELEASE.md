RELEASE_TYPE: patch

This patch improves the performance of text generators that are constructed inside the test body, which is how most tests write them. Building a text generator's alphabet from its codec, codepoint bounds, Unicode categories and included or excluded characters cost more than the draws made from it, and was repeated for every test case; the engine now shares the built alphabet between generators with the same constraints, so a test drawing one short string now runs in about half the instructions per test case.
