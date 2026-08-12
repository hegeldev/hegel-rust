RELEASE_TYPE: patch

This patch significantly improves shrinking for stateful tests that use `stateful::Pool`. Pool draws are now recorded as stable variable identifiers rather than indices into the pool's current contents, so removing irrelevant rules during shrinking no longer changes which pooled value later rules act on, and shrunk failing rule sequences are usually minimal.
