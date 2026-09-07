RELEASE_TYPE: patch

This patch improves how the failure database is maintained. New entries are saved before the entries they supersede are removed, so interrupting a run mid-shrink can no longer lose a failure. A shrink no longer deposits its chain of intermediate improvements into the secondary corpus, and the secondary corpus is capped at 50 entries. An entry whose bytes still serve as another failure's latest save is never deleted.
