RELEASE_TYPE: patch

This patch fixes a silent size collapse in recursively generated values. The nested-span-depth guard (100) was within reach of legitimate generation — a recursive generator opens several spans per recursion level — and values that crossed it were concluded invalid mid-generation, collapsing typical sizes by roughly 7x for grammars written as a choice over operator arms with tuple-drawn subtrees. The cap is now 1000: far above legitimate depths, still low enough to catch runaway recursion before it overflows the stack.
