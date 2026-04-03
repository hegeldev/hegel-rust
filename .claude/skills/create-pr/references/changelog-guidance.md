# Changelog Style Guide

This guide describes the style for writing `RELEASE.md` files for hegel-rust. The style is modeled on the [Hypothesis changelog](https://hypothesis.readthedocs.io/en/latest/changes.html).

## Opening sentence pattern

Every entry should open with a sentence that signals the scope and nature of the change:

- **Small fixes/improvements (patch):** Start with `"This patch ..."`
- **Larger changes (minor):** Start with `"This release ..."`
- **Tiny internal-only changes:** A bare sentence is fine — `"Internal refactoring."` or `"Clean up some internal code."`

The opening verb should tell the reader what *kind* of change this is:

| Change type | Opening pattern |
|---|---|
| Bug fix | `"This patch fixes ..."` or `"Fix ..."` |
| New feature | `"This release adds ..."` |
| Improvement | `"This patch improves ..."` or `"This release makes ... more ..."` |
| Deprecation | `"This release deprecates ..."` |
| Breaking change | `"This release changes ..."` (then explain migration) |
| Performance | `"This patch improves the performance of ..."` or `"Optimize ..."` |
| Internal-only | `"Internal refactoring."` / `"Refactor some internals."` / `"Clean up some internal code."` |

## Describe the user impact, not the implementation

Bad: `"Refactored the socket handling code to use a shared connection pool."`

Good: `"This release changes the way the client manages the server to run a single persistent process for the whole test run. This should improve the performance of running many hegel tests."`

For **bug fixes**, describe the bug (what went wrong from the user's perspective), not just "fixed a bug":

Bad: `"Fix bug in server crash detection."`

Good: `"Fix server crash detection. The client now properly detects when the hegel server process exits unexpectedly, instead of hanging indefinitely."`

## Length calibration

- **Internal-only changes:** 1 sentence. (`"Refactor some internals."`)
- **Simple bug fixes:** 1-3 sentences. Describe the bug and what changed.
- **New features:** 1-2 short paragraphs. Describe what it does and why it's useful.
- **Breaking changes / API changes:** Multiple paragraphs. Include before/after code examples and migration guidance.

Don't pad entries. If a change can be described in one sentence, use one sentence.

## Code examples

Include fenced code blocks for:
- New API features (show usage)
- Breaking changes (show before/after)
- Anything where seeing the code is clearer than describing it

Don't include code blocks for bug fixes or internal changes.

## References

- Reference GitHub issues when relevant: `([#123](https://github.com/hegeldev/hegel-rust/issues/123))`
- Reference previous versions when building on prior work
- Reference related libraries/specs when relevant

## Tone

- Third person, present tense for describing behavior
- Professional but conversational — be direct, not formal
- Honest about uncertainty: `"This should improve performance"`, `"We expect this to..."`, `"In some cases this may..."`
- It's okay to briefly explain *why* a change was made if the motivation isn't obvious

## Things to avoid

- No emojis
- No bullet lists for single-topic entries (use them for multi-topic entries like API cleanups)
- No commit hashes or PR numbers in the text (issue numbers are fine)
- Don't describe the implementation when you can describe the effect
- Don't use vague language like `"various improvements"` — be specific about what changed
- Don't add marketing language or hype

## Examples

**Good patch (bug fix):**
```
RELEASE_TYPE: patch

This patch fixes `generators::hashsets` generating duplicate elements in rare cases when the element generator had a very small output space.
```

**Good patch (internal):**
```
RELEASE_TYPE: patch

Internal refactoring of the protocol handling code.
```

**Good minor (new feature):**
```
RELEASE_TYPE: minor

This release adds support for `HealthCheck`. A health check is a proactive error raised by Hegel when we detect your test is likely to have degraded testing power or performance. For example, `FilterTooMuch` is raised when too many test cases are filtered out by the rejection sampling of `.filter()` or `assume()`.

Health checks can be suppressed with the new `suppress_health_check` setting.
```

**Good major (breaking change):**
```
RELEASE_TYPE: major

This release changes `self` in `#[invariant]` from an immutable reference to a mutable reference:

\```rust
# before
fn my_invariant(&self, ...) {}

# after
fn my_invariant(&mut self, ...) {}
\```

This will require updating your invariant signatures, but should be strictly more expressive.
```
