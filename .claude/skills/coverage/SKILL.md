---
name: coverage
description: "How to approach code coverage in this project. Use when coverage CI fails, when writing tests for new code, when deciding whether to add // nocov, or when you need to make untestable code testable. Also use proactively when writing new code to ensure it will be coverable."
---

# Code Coverage

This project requires 100% line coverage for new code, with explicit nocov annotations only allowed under rare circumstances and with human permission.

This is implemented as a ratchet which counts the number of lines annotated as not requiring coverage. If the number of excluded lines exceeds the ratchet value, or if there are any uncovered lines without a `// nocov` annotation, the coverage check fails.

## The ratchet is not a budget

The nocov count in `.github/coverage-ratchet.json` tracks excluded lines and can only decrease. Just because previous work reduced the count does not mean you have implicit permission to add new uncovered lines. Think of the ratchet as immediately ratcheting down after any reduction — the slack is gone.

You may not add `// nocov` annotations without explicit human permission. If you think code is genuinely untestable, your first move should be to refactor it for testability, not to annotate it. See `references/patterns.md` for testability refactoring examples.

## Writing good tests

Tests should catch real bugs, not mirror the implementation.

- **Validate against independent sources**: if you can obtain the correct answer some other way - e.g. checking it against an externally defined source, or calculating it through some simpler more expensive method - then you should validate against that in the test.
- **Think in terms of what could go wrong**: a test that would still pass after introducing a bug is not testing anything. Figure out ways in which the code could genuinely be wrong and write tests that would catch that if it were.

## Diagnosing coverage failures

When CI coverage fails:

1. Read the failure output — it lists each uncovered file:line and content.
2. Categorize each uncovered line:
   - **Just needs testing**: most code that has not been covered just needs a test written for it. Your default assumption should be that it is straightforwardly testable and you just need to write a normal test.
   - **Needs refactoring**: some code cannot easily be tested in its current form and needs refactoring to make it testable. See `references/patterns.md` for information on how to do that.
   - **Dead code**: if it's truly unreachable, delete it, or replace it with an `unreachable!` annotation.
3. Run `just check-coverage` locally if `cargo-llvm-cov` is available to iterate faster than CI.

For details on how the coverage script works and what it auto-excludes, see `references/internals.md`.
