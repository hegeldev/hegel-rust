RELEASE_TYPE: patch

Tests would hang if you were using an old version of hegel-core that didn't support the --stdio flag. This fixes that and adds some comprehensive debugging messages when the server start doesn't work.
