RELEASE_TYPE: patch

This patch significantly improves shrinking for stateful tests that use `stateful::Pool`. Pool draws are now recorded as stable variable identifiers rather than indices into the pool's current contents, so removing irrelevant rules during shrinking no longer changes which pooled value later rules act on, and shrunk failing rule sequences are usually minimal.

This changes the recorded choice sequence of pool draws, so previously saved database entries and `reproduce_failure` blobs for tests using pools no longer reproduce their failures; stale database entries are discarded automatically, while `reproduce_failure` blobs fail loudly and should be regenerated.
