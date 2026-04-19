# Coverage Script Internals

This file describes how "100% coverage" is actually measured. It should be, but may not have been, kept up to date.

The canonical source of truth for the behaviour is `scripts/check-coverage.py`, and if you notice discrepancies between what you see here and what is happening, check the script and, if necessary, update this file with any changes.

## How the coverage script works

1. Runs `cargo llvm-cov --no-report --features rand,antithesis` (standard) or `--features native` (native mode) to collect coverage data. Code under `#[cfg(not(feature = "antithesis"))]` is not compiled in the standard run and will not appear in the coverage output.
2. Generates an LCOV report. Tries to include TempRustProject subprocess binaries (found as `temp_hegel_test_*` in the target directory), though this depends on the binaries being compiled with coverage instrumentation (they inherit `RUSTFLAGS` from the `cargo llvm-cov` parent process, which should instrument them).
3. Parses the LCOV data (`DA:<line>,<count>` format — line-level, not region-level).
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
