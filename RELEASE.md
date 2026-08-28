RELEASE_TYPE: patch

This patch fixes `generators::recursive` generating far smaller values than `max_leaves` allows. The branching probability assumed every branch draws exactly two sub-values, so grammars averaging fewer (e.g. an expression grammar with more unary than binary operators) collapsed to a handful of nodes; and even at a correct probability, typical sizes stay tiny — the size distribution it induces is heavy-tailed with a median of a few nodes no matter how it is tuned.

Each draw now samples a target size from across the leaf budget and steers generation toward it, adapting to the number of sub-values the branch function actually draws. Typical sizes now span the whole range up to `max_leaves`, and grammars that can never grow past one leaf (chains of unary operators) spread over the whole depth range up to `max_depth`.
