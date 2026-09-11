RELEASE_TYPE: patch

This patch changes how releases are tagged in the hegel-rust repository. Every `hegeltest` release is now tagged `v<version>`, and libhegel releases, which previously took the plain `v<version>` tags, are tagged `libhegel-v<version>` instead. Nothing about the crate itself changes.
