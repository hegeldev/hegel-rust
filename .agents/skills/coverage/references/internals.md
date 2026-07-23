# Coverage Script Internals

This file describes how "100% coverage" is actually measured. It should be, but may not have been, kept up to date.

The canonical source of truth for the behaviour is `scripts/check-coverage.py`, and if you notice discrepancies between what you see here and what is happening, check the script and, if necessary, update this file with any changes.

## How the coverage script works

1. Runs `cargo llvm-cov --workspace` (with every additive feature) to collect coverage data for both crates, emitting LCOV.
2. Runs a second `cargo llvm-cov -p hegeltest-c --lib` phase in a hermetic target directory (`target/coverage-hegel-c-lib`) and union-merges the two LCOV files per line. This is a correctness requirement, not redundancy: the workspace pass links two compilations of hegel-c (the shared rlib and the crate's own `--test` build), and every `#[no_mangle] hegel_*` function has the same coverage-record *name* but a different record *hash* in each — `llvm-cov` silently drops one side's counts, so a line inside a `no_mangle` function covered only by hegel-c's embedded tests would deterministically show as uncovered. The isolated phase sees only the lib test's own object (guaranteed by the hermetic target directory), where those counts resolve correctly; merging at the LCOV line level is immune to the collision. Mangled functions are unaffected (unique record names per compilation).
3. Parses the merged LCOV data (`DA:<line>,<count>` format — line-level, not region-level).
4. Checks each uncovered line against automatic exclusion patterns.
5. Lines that don't match any exclusion and don't have `// nocov` are reported as failures.
6. Automatically removes `// nocov` from lines that turn out to be covered, keeping the annotation count honest.

## Automatic exclusions

These patterns are excluded without needing `// nocov`:

- Structural syntax (closing braces, punctuation-only lines)
- `#[cfg(test)]` modules
- `todo!()` and `unreachable!()` calls
- Continuation lines inside multi-line `unreachable!()`/`todo!()`/`assert!()` calls
- `#[ignore]`d test bodies

## LCOV vs region coverage

`cargo llvm-cov --text` shows region-level coverage (the `^0` markers inside lines). The coverage script uses LCOV which is **line-level** — a line with execution count > 0 is covered even if some closure bodies within it weren't executed. This means `map_err` closures on covered lines don't need separate tests.
