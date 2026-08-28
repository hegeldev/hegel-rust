RELEASE_TYPE: patch

This patch fixes the distribution of `generators::recursive` to cover a broader range of leaves up to `max_leaves`. Previously for some uses (especially ones where the branch case often drew 0 or 1 leaves) the recursive generator ended up biased very heavily towards small values.
