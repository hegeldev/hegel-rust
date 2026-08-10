RELEASE_TYPE: patch

This patch improves random generation for tests with repeated structure (recursive generators, collections, state machines). The engine proposes new test cases by splicing the choices of one span over another with the same label; a spliced sequence that diverged from its donor's path was previously discarded, and is now completed with fresh random draws instead, so every such proposal becomes a real test case seeded with the mutation.
