RELEASE_TYPE: patch

This patch fixes two ways the native engine diverged from Hypothesis (and the server backend) when generating from a high-rejection-rate test, both reported by Ethan.

The first: a test that filters out almost everything used to keep generating until it had run ten times as many test cases as the configured budget, regardless of how few were valid. It now stops once the number of rejected (and overrunning) inputs exceeds Hypothesis's invalid budget — `458 + 100 * valid_examples` — so a test that never produces a valid input gives up after 459 cases instead of `10 * test_cases`.

The second: the `FilterTooMuch` health check now fires after 50 total rejected inputs while fewer than 10 valid inputs have been seen, matching Hypothesis. Previously it required 200 *consecutive* rejections and could never fire once a single valid input had been generated. Inputs that overrun (for example a collection drawing many unique values from a small pool) count toward the generation budget but, as in Hypothesis, no longer count toward `FilterTooMuch`.
