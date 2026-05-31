RELEASE_TYPE: patch

The native backend (`--features native`) now rejects the regex `\z` anchor, matching CPython's `re` (which only supports `\Z`).
