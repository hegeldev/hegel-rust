RELEASE_TYPE: patch

This patch fixes a use-after-free: a `TestCase` moved to a thread that outlived its test could touch freed memory when it drew after the test had finished. Such a draw now fails with a clear panic in that thread instead. As before, any thread that draws should be joined before the test returns.
