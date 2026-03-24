RELEASE_TYPE: patch

This patch adds support for setting `seed` to the protocol.

---

Every pull request which modifies the source code must include a `RELEASE.md` file. This `RELEASE-sample.md` file is an example of that file.

In the example above, "patch" on the first line should be replaced by "minor" if changes are visible in the public API, or "major" if there are breaking changes.  Note that only maintainers should ever make a major release.

The remaining lines are the actual changelog text for this release, which should:

- concisely describe any public-facing changes, and why. Internal-only changes can be documented as e.g. "This release improves an internal invariant."
- use `single backticks` for verbatim code.

After the pull request is merged, the contents of this file (except the first line) are automatically added to `CHANGELOG.md`. More examples can be found in that file.
