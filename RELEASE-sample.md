RELEASE_TYPE: patch

This patch adds support for setting `seed` to the protocol.

---

Every pull request which modifies the source code must include a `RELEASE.md` file. This `RELEASE-sample.md` file is an example of that file.

Changes to the hegel-rust crate go in this root `RELEASE.md`; changes to the libhegel C ABI go in `hegel-c/RELEASE.md`, which feeds `hegel-c/CHANGELOG.md`. The two crates version independently. When a PR releases hegel-c but makes no user-facing change to hegel-rust, the root entry is auto-generated as a dependency bump and you write only the `hegel-c/RELEASE.md` — but that's a judgment call, not an automatic one. See the `changelog` skill for details.

While we are on a `0.x` major version the semver levels are shifted: **breaking changes are "minor"** and **everything else (bug fixes, internal changes, and new non-breaking features / API additions) is "patch"**. So in the example above, "patch" on the first line should be replaced by "minor" only for a breaking change. "major" is reserved for the eventual 1.0 and beyond, and only maintainers should ever make a major release.

The remaining lines are the actual changelog text for this release, which should:

- concisely describe any public-facing changes, and why. Internal-only changes can be documented as e.g. "This release improves an internal invariant."
- use `single backticks` for verbatim code.

After the pull request is merged, the contents of this file (except the first line) are automatically added to `CHANGELOG.md`. More examples can be found in that file.
