RELEASE_TYPE: patch

Enable the `#![forbid(future_incompatible)]` and `#![cfg_attr(docsrs, feature(doc_cfg))]` attributes, the latter of which unblocks our docs.rs build.
