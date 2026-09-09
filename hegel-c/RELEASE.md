RELEASE_TYPE: patch

This patch replaces the engine's data tree with a flat execution cache. Runs get faster — recording overhead on passing workloads drops substantially and stateful shrinking speeds up by about 40% — while the tree's main benefit, serving repeated shrink probes from memory, is kept. Cache memory is bounded at 8 MiB where the tree grew without limit.

Some behaviour moves with it. A test that repeatedly produces the same values but flips between passing and failing is now detected and fails the run as flaky, where the tree silently kept its first conclusion. Exhaustible generation spaces no longer stop the run early: `FilterTooMuch` now fires after a streak of duplicate invalid test cases, and a tiny valid space runs to the normal test-case budget. Recursive generators reach extreme depths somewhat less often.
