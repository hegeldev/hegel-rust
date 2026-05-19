RELEASE_TYPE: patch

This release improves the native backend's shrinker: a new `lower_common_node_offset` pass jointly lowers chains of integer choices that no individual or pairwise pass can move on its own, and `try_shortening_via_increment` now offers float-typed candidates instead of letting its powers-of-2 fallback be silently dead code for floats.
