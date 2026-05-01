RELEASE_TYPE: patch

This patch fixes `vecs(...).unique(true)` producing duplicate elements when the element generator is `sampled_from`. The server-side uniqueness check was enforcing index-level uniqueness (distinct indices into the pool), not value-level uniqueness, so pools with repeated values could produce duplicates. Unique vecs now always use the client-side uniqueness path that checks actual values.
