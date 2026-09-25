RELEASE_TYPE: patch

This patch improves the performance of text generators that are constructed inside the test body, which is how most tests write them. Building a text generator's alphabet from its codec, codepoint bounds, Unicode categories and included or excluded characters cost more than the draws made from it, and was repeated for every test case; the engine now shares the built alphabet between generators with the same constraints, so a test drawing one short string now runs in about half the instructions per test case.

This patch also trims the fixed cost of every test case. After each case the engine no longer copies the case's choices and spans out of a test case nothing else can still see, hands the driver its data source without re-allocating it, and only indexes spans by label for span mutation when some label actually repeats. A test drawing a single value runs about 15% fewer instructions per test case, and shrinking runs 3–10% fewer.
