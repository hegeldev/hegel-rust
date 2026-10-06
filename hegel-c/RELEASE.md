RELEASE_TYPE: minor

This release changes `hegel_string_generator_regex` to take its pattern as a UTF-8 buffer with an explicit byte length, `const uint8_t *pattern, size_t pattern_len`, in place of a NUL-terminated `const char *`. Python's `re` accepts the NUL character in a pattern, and a NUL-terminated string could not carry it, so bindings had to substitute U+FFFD and the generated strings did not match the pattern. Bindings pass the length at their one call site; the other string-generator constructors are unchanged.

It also fixes four ways the regex generator could produce a string that does not match its pattern as Python's `re` reads it, or reject a string it could have produced:

- Under `(?i)`, characters are now compared the way Python does — by their simple lowercase mapping, plus the extra equivalences of `re._casefix` such as `s` and `ſ` — so `(?i)[^k]` no longer generates the Kelvin sign `K`, `(?i)[^İ]` no longer generates `I`, and `(?i)k` can generate every character Python accepts for it.
- Under `(?a)`, case-insensitivity folds only ASCII letters, so `(?ai)À` generates `À` and never `à`, and `\d`, `\s`, `\w` and `\b` use their ASCII definitions. This also covers bytes patterns, which bindings pass with `(?a)`.
- Under `(?a)`, explicit characters and ranges outside ASCII are kept rather than dropped, so `(?a)[Ï-İ]` and `(?a)[^\x00-\xff]` generate strings instead of rejecting every attempt.
- A repetition whose body the alphabet cannot supply is generated zero times, and an alternation chooses only among the branches the alphabet can supply, instead of rejecting the attempt. `(?-i:Ā)*k` over an alphabet of code points up to 127 generates `k` rather than tripping the filter-too-much health check.
