RELEASE_TYPE: patch

This patch improves the performance of generating and shrinking bounded integers, and of any generator built on them (collection sizes, sampled-from indices, and similar). The mapping from a choice's shrink-order position back to its value is now computed in closed form instead of by binary search, removing a per-draw cost that grew with the width of the integer's range.
