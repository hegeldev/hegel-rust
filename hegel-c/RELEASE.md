RELEASE_TYPE: patch

This patch lets C callers pass `NULL` with a zero byte length to `hegel_string_generator_regex` to match an empty pattern. A `NULL` string buffer with a nonzero length now returns an invalid argument error.
