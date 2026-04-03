---
name: self-review
description: "Review your own changes before creating a pull request. Use this before running create-pr, or whenever you want to check that your changes are clean, correct, and well-tested. Trigger when: about to create a PR, finished a chunk of work, or asked to review changes."
---

# Self-Review

Run through these checks before creating a PR or declaring work complete. The goal is to catch the things a human reviewer would flag, before they have to.

## Mechanical checks

Run all of these — they're fast and catch the most common issues:

```bash
just check   # formatting, lint, tests, docs
```

Fix any failures before proceeding. Don't skip checks because they passed locally last time — the code may have changed.

## Review the diff

Run `git diff origin/main...HEAD` and read through it. Look for:

- **Dead code**: unused imports, variables, functions that were left behind during refactoring.
- **Redundant test logic**: tests that just mirror the implementation (e.g., a test with the same match arms as the code it's testing). Good tests validate against external reality or test behavior, not structure. If a test would still pass after introducing a bug in the code, it's not testing anything useful.
- **Network calls in tests**: can any test that hits the network use `file://` URLs or local fixtures instead? Keep at most one integration test that verifies the real network path works.
- **Hardcoded paths**: `/tmp/foo` in tests should be `tempfile::tempdir()` for isolation and cleanup.
- **Missing error context**: error messages should help the user fix the problem (e.g., include "Install X manually: <url>").
- **New `// nocov` annotations**: each one needs explicit human permission. Your first instinct should be to write a test or refactor for testability instead.

## Coverage check

If `cargo-llvm-cov` is available, run `just check-coverage` and review the output. If new code is uncovered, address it before the PR — don't rely on CI to catch it. See the `coverage` skill for how to approach coverage gaps.

## Final gut check

Ask yourself: if someone else wrote this diff, what would I flag in code review? Common things to catch:
- Functions that panic where they should return errors (or vice versa)
- Thin wrapper functions that exist only for testability but aren't actually tested
- Comments that describe *what* the code does rather than *why*
