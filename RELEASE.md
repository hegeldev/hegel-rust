RELEASE_TYPE: patch

This patch makes every test case a little cheaper to run on the hegel-rust side. Together with this release's engine changes, tests that draw a single value run 5–10% faster and tests that draw many values 27–46% faster.
