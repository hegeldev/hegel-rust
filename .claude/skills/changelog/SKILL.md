---
name: changelog
description: "Changelog style guide for writing RELEASE.md files. Use when creating or reviewing RELEASE.md, writing changelog entries, or preparing a PR that needs release notes."
---

# Changelog Style Guide

This guide describes the style for writing `RELEASE.md` files for hegel-rust. The style is modeled on the [Hypothesis changelog](https://hypothesis.readthedocs.io/en/latest/changes.html).

## Choosing `RELEASE_TYPE`

hegel-rust is currently zerover (`0.x.y`), so the usual semver mapping does **not** apply. While we are pre-1.0:

- **`patch`** — Bug fixes, internal changes, **and new features / non-breaking API additions**. The default choice.
- **`minor`** — **Breaking changes only.** Any change that requires users to update their code (renamed/removed APIs, changed signatures, behavior changes that could break downstream tests) is a minor bump.
- **`major`** — Not used while we are zerover. Reserve for the eventual 1.0 and beyond.

If you find yourself reaching for `minor` because the change feels "big," check whether it actually breaks any caller. A large new feature that adds API surface without removing or changing existing behavior is still a `patch`.

## Opening sentence pattern

Every entry should open with a sentence that signals the scope and nature of the change:

- **Patch (fixes, improvements, new features):** Start with `"This patch ..."`
- **Minor (breaking changes):** Start with `"This release ..."` and explain migration
- **Tiny internal-only changes:** A bare sentence is fine — `"Internal refactoring."` or `"Clean up some internal code."`

The opening verb should tell the reader what *kind* of change this is:

| Change type | RELEASE_TYPE | Opening pattern |
|---|---|---|
| Bug fix | `patch` | `"This patch fixes ..."` or `"Fix ..."` |
| New feature | `patch` | `"This patch adds ..."` |
| Improvement | `patch` | `"This patch improves ..."` |
| Performance | `patch` | `"This patch improves the performance of ..."` or `"Optimize ..."` |
| Deprecation | `minor` | `"This release deprecates ..."` |
| Breaking change | `minor` | `"This release changes ..."` (then explain migration) |
| Internal-only | `patch` | `"Internal refactoring."` / `"Refactor some internals."` / `"Clean up some internal code."` |

## Describe the user impact, not the implementation

Bad: `"Refactored the socket handling code to use a shared connection pool."`

Good: `"This patch changes the way the client manages the server to run a single persistent process for the whole test run. This should improve the performance of running many hegel tests."`

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

**Good patch (new feature):**
```
RELEASE_TYPE: patch

This patch adds support for `HealthCheck`. A health check is a proactive error raised by Hegel when we detect your test is likely to have degraded testing power or performance. For example, `FilterTooMuch` is raised when too many test cases are filtered out by the rejection sampling of `.filter()` or `assume()`.

Health checks can be suppressed with the new `suppress_health_check` setting.
```

**Good minor (breaking change):**
```
RELEASE_TYPE: minor

This release changes `self` in `#[invariant]` from an immutable reference to a mutable reference:

\```rust
# before
fn my_invariant(&self, ...) {}

# after
fn my_invariant(&mut self, ...) {}
\```

This will require updating your invariant signatures, but should be strictly more expressive.
```
