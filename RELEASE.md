RELEASE_TYPE: patch

This release should have minimal user-facing impact. However, in
some rare cases you may see generation get work. Specifically,
some special handling for generators with small domains has been
removed. This can show up when generating unique collections
such as sets from e.g. `gs::booleans` or `gs::sampled_from`,
or when filtering from such. These will now sometimes error
with a `FilterTooMuch` health check in cases they previously
would have worked.

If you run into this problem in practice, please file a bug report.
