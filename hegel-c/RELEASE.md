RELEASE_TYPE: patch

This patch improves shrinking for stateful tests that use variable pools. `hegel_pool_add` now draws each variable id from the test case's stream as a fresh identifier recorded by value, and `hegel_pool_generate` records the chosen id itself rather than an index into the pool's current contents. Deleting a pool addition during shrinking no longer shifts what every later pool draw refers to, so shrinking can remove irrelevant additions and reliably reaches minimal rule sequences.
