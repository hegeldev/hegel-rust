RELEASE_TYPE: patch

This patch fixes several ways `from_regex` could generate a string that does not match its pattern as Python's `re` reads it, or reject a string it could have generated:

- A NUL character in the pattern was replaced by U+FFFD on the way to the engine, so `from_regex("a\0b")` generated `"a\u{FFFD}b"`. The pattern now reaches the engine intact and generates `"a\0b"`.
- Under `(?i)`, characters are now compared the way Python does — by their simple lowercase mapping, plus Python's table of extra equivalences such as `s` and `ſ` — so `(?i)[^k]` no longer generates the Kelvin sign `K`, `(?i)[^İ]` no longer generates `I`, and `(?i)k` can generate every character Python accepts for it.
- Under `(?a)`, case-insensitivity now folds only ASCII letters, so `(?ai)À` generates `À` and never `à`; `\d`, `\s`, `\w` and `\b` use their ASCII definitions; and explicit characters and ranges outside ASCII are kept rather than dropped, so `(?a)[Ï-İ]` and `(?a)[^\x00-\xff]` generate strings instead of rejecting every attempt.
- A repetition whose body the alphabet cannot supply is now generated zero times, and an alternation chooses only among the branches the alphabet can supply, instead of rejecting the attempt. `(?-i:Ā)*k` with `.alphabet(gs::characters().max_codepoint(127))` generates `k` rather than tripping `FilterTooMuch`.
