RELEASE_TYPE: patch

This patch adds support for setting `seed` to the protocol.

---

Every pull request which modifies the source code must include a `RELEASE.md` file. This `RELEASE-sample.md` file is an example of that file.

Changes to the hegel-rust crate (`src/`, `hegel-macros/`) require this root `RELEASE.md`. Changes to the libhegel C ABI (`hegel-c/src/`) require a `hegel-c/RELEASE.md` instead, which feeds `hegel-c/CHANGELOG.md`. A PR that changes both needs both files, in the same format. See the `changelog` skill for details, including how a C-ABI-only change auto-generates the root changelog entry.

In the example above, "patch" on the first line should be replaced by "minor" if changes are visible in the public API, or "major" if there are breaking changes.  Note that only maintainers should ever make a major release.

The remaining lines are the actual changelog text for this release, which should:

- concisely describe any public-facing changes, and why. Internal-only changes can be documented as e.g. "This release improves an internal invariant."
- use `single backticks` for verbatim code.

After the pull request is merged, the contents of this file (except the first line) are automatically added to `CHANGELOG.md`. More examples can be found in that file.
