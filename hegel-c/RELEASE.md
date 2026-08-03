RELEASE_TYPE: patch

This patch improves the value distribution of bounded integer generation for large and full-width ranges, so that property tests surface off-by-one, overflow, and sign bugs far more readily.

Drawing an integer from a wide range now chooses between four categories of value — the range endpoints and their inner neighbours (`min`, `max`, `min + 1`, `max - 1`); the curated "interesting" values (zero, ±1, small magnitudes, powers of two, type limits); the diffuse pool of large constants; and the ordinary middle of the range — using a mixture whose weights are drawn afresh for each test case from a Dirichlet distribution (a form of *swarm testing*, after Groce et al., ISSTA 2012). Most test cases stay middle-dominated, so ordinary values remain the common case, while a lumpy minority concentrate on one special category.

Splitting the range endpoints into their own category, and drawing the weights per test case rather than fixing them, is what makes interactions that need *several* operands to be extreme at once reachable. Both operands of an expression drawn in an endpoint-heavy case land on `{min, max, …}` together, so — measured over 20,000 full-width `i64` cases — `x + y` now overflows about 1.4% of the time, where a fixed per-value boundary probability would reach it only at roughly its square. A boundary value appears about 2% of draws (up from about 0.3% before) and a small value about 3% (up from about 0.6%), now clustered in the cases that exercise them rather than spread thinly across every draw.

The special values still shrink toward zero and the simplest endpoints, and narrow ranges are unaffected. Mirrors the boundary-value injection Hypothesis performs (see [#350](https://github.com/hegeldev/hegel-rust/issues/350) and hypothesis#4722).
