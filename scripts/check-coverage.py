#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.10"
# ///
"""
Custom Code Coverage Check Script
=================================

PURPOSE:
This script provides a more specific coverage check than simple percentage thresholds.
LLVM's coverage instrumentation counts regions which sometimes includes closing
braces of control structures as separate coverage points. Additionally, certain
patterns like todo!() and unreachable!() are allowed to be uncovered.

ALLOWED UNCOVERED PATTERNS:
---------------------------

1. Structural Syntax (closing braces, etc.)
   LLVM-cov reports closing braces as uncovered due to region-based tracking.
   Lines containing only: } ) ; , or combinations thereof

2. todo!() Placeholders
   Lines that are just todo!() or todo!("message") represent intentionally
   unimplemented code that will panic if called.

3. unreachable!() Placeholders
   Lines that are just unreachable!() or unreachable!("message") mark code
   paths that should never be reached.

4. Multi-line macro continuations
   Continuation lines inside multi-line unreachable!() or todo!()
   calls (e.g. format string arguments).

5. #[ignore]d test bodies
   LLVM-cov marks the body of #[ignore]d tests as uncovered because
   the test framework compiles them but never runs them.

6. // nocov Annotations
   Lines marked with // nocov are manually excluded from line coverage.
   Block exclusions with // nocov start ... // nocov end are also supported.
   These are tracked by a ratchet mechanism -- the count can only decrease.

RATCHET MECHANISM:
------------------
The number of lines excluded via // nocov is tracked in .github/coverage-ratchet.json.
This count may only decrease over time. If coverage analysis reveals that a
line-level annotation is no longer needed (the code is now covered), the
annotation is automatically removed.

SECURITY NOTE:
--------------
This script should only be modified to allow ADDITIONAL exclusions with EXTREME CAUTION.
Adding new patterns to the allowlist could mask actual coverage gaps.
"""

from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
from dataclasses import dataclass, field
from pathlib import Path

RATCHET_FILE = Path(".github/coverage-ratchet.json")
SOURCE_DIRS = [Path("src"), Path("hegel-macros/src")]

# ──────────────────────────────────────────────────────────────────────
# nocov block cache
# ──────────────────────────────────────────────────────────────────────

_nocov_block_cache: dict[Path, set[int]] = {}


def get_nocov_block_lines(file_path: Path) -> set[int]:
    """Return the set of line numbers inside // nocov start ... // nocov end blocks."""
    resolved = file_path.resolve()
    if resolved in _nocov_block_cache:
        return _nocov_block_cache[resolved]

    excluded: set[int] = set()
    in_block = False
    try:
        with file_path.open() as f:
            for i, line in enumerate(f, 1):
                if re.search(r"//\s*nocov\s+start\b", line):
                    in_block = True
                    continue
                if re.search(r"//\s*nocov\s+end\b", line):
                    in_block = False
                    continue
                if in_block:
                    excluded.add(i)
    except (OSError, IOError):
        pass

    _nocov_block_cache[resolved] = excluded
    return excluded


# ──────────────────────────────────────────────────────────────────────
# Data structures
# ──────────────────────────────────────────────────────────────────────


@dataclass
class UncoveredLine:
    """Represents an uncovered line in the source code."""

    file: Path
    line_number: int
    content: str

    def is_structural_syntax_only(self) -> bool:
        """
        Check if this line contains only structural syntax (closing braces, etc.).

        Returns True for lines like: }  })  },  });  }};  etc.
        Returns False for: }else  } // comment  }foo  or any actual code
        """
        stripped = self.content.strip()
        # Remove all structural characters and whitespace
        cleaned = re.sub(r"[})\];,\s]", "", stripped)

        # If nothing remains after removing structural chars, it is just syntax
        # But the original must have had something (not be empty)
        return len(cleaned) == 0 and len(stripped) > 0

    def is_todo_placeholder(self) -> bool:
        """
        Check if this line is just a todo!() placeholder.

        todo!() is used for intentionally unimplemented code that will panic
        if called. These are expected to be uncovered in tests.

        Returns True for lines like: todo!()  todo!("message")  todo!();
        """
        stripped = self.content.strip()
        return bool(re.match(r"^todo!\s*\([^)]*\)\s*;?\s*$", stripped))

    def is_unreachable_placeholder(self) -> bool:
        """
        Check if this line contains unreachable!() as the primary action.

        Matches standalone unreachable!() and match-arm unreachable!():
            unreachable!("msg")
            _ => unreachable!("msg"),
            unreachable!(            // multi-line start
        """
        stripped = self.content.strip()
        if re.search(r"unreachable!\s*\(.*\)\s*[;,]?\s*$", stripped):
            return True
        if re.search(r"unreachable!\s*\(", stripped) and not stripped.rstrip(
            ";"
        ).rstrip().endswith(")"):
            return True
        return False

    def is_inside_excluded_macro(self) -> bool:
        """
        Check if this line is a continuation of a multi-line unreachable!/todo!()/assert!().

        Looks backward for an unclosed macro call that spans multiple lines.
        """
        try:
            with self.file.open() as f:
                lines = f.readlines()
        except (OSError, IOError):
            return False

        idx = self.line_number - 1  # 0-indexed

        # Look backward for a macro call start within 20 lines
        for start in range(idx - 1, max(idx - 20, -1), -1):
            if start < 0 or start >= len(lines):
                continue
            line = lines[start].strip()
            if re.search(r"\b(unreachable|todo|assert)!\s*\(", line):
                # Count parens from macro start to line BEFORE current
                paren_depth = 0
                for check_idx in range(start, idx):
                    for ch in lines[check_idx]:
                        if ch == "(":
                            paren_depth += 1
                        elif ch == ")":
                            paren_depth -= 1
                # If still open, current line is inside the macro call
                if paren_depth > 0:
                    return True
                # Macro closed before our line -- stop searching
                return False
        return False

    def has_nocov_annotation(self) -> bool:
        """
        Check if this line is excluded by a // nocov annotation.

        Matches both:
          - Inline: code(); // nocov
          - Block:  inside a // nocov start ... // nocov end region
        """
        # Any line containing // nocov (inline, start, or end marker)
        if re.search(r"//\s*nocov\b", self.content):
            return True
        # Inside a block annotation
        return self.line_number in get_nocov_block_lines(self.file)

    def is_test_code(self) -> bool:
        """
        Check if this line is inside a #[cfg(test)] module.

        Test code coverage is not meaningful -- we care about coverage of
        production code, not test helpers.

        Scans backward from this line looking for #[cfg(test)].
        """
        try:
            with self.file.open() as f:
                lines = f.readlines()
        except (OSError, IOError):
            return False

        idx = self.line_number - 1
        brace_depth = 0
        for i in range(idx, -1, -1):
            line = lines[i].strip() if i < len(lines) else ""
            brace_depth += line.count("}")
            brace_depth -= line.count("{")
            if brace_depth < 0:
                for j in range(i, max(i - 5, -1), -1):
                    check_line = lines[j].strip() if j < len(lines) else ""
                    if "#[cfg(test)]" in check_line:
                        return True
                brace_depth = 0
        return False

    def is_ignored_test_body(self) -> bool:
        """
        Check if this line is inside an #[ignore]d test function.

        LLVM-cov marks the body of #[ignore]d tests as uncovered because
        the test framework compiles them but never runs them.
        """
        try:
            with self.file.open() as f:
                lines = f.readlines()
        except (OSError, IOError):
            return False

        idx = self.line_number - 1
        for i in range(idx, max(idx - 50, -1), -1):
            line = lines[i].strip() if i < len(lines) else ""
            if line.startswith("fn "):
                for j in range(i - 1, max(i - 4, -1), -1):
                    attr = lines[j].strip() if j < len(lines) else ""
                    if attr.startswith("#[ignore"):
                        return True
                return False
        return False


@dataclass
class CoverageData:
    """Full parsed coverage data from LCOV."""

    uncovered_lines: list[UncoveredLine] = field(default_factory=list)
    # Lines with execution count > 0
    covered_lines: dict[Path, set[int]] = field(default_factory=dict)


# ──────────────────────────────────────────────────────────────────────
# Coverage execution
# ──────────────────────────────────────────────────────────────────────


def get_target_triple() -> str:
    """Get the current Rust target triple."""
    result = subprocess.run(
        ["rustc", "-vV"],
        capture_output=True,
        text=True,
    )
    for line in result.stdout.splitlines():
        if line.startswith("host:"):
            return line.split(":")[1].strip()
    return "unknown"


def run_coverage(native_mode: bool = False) -> Path:
    """Run coverage analysis and generate LCOV report.

    When native_mode is True, runs with --features native (the native backend).
    When False, runs with --features rand,antithesis (excludes native).
    """
    mode_label = "native" if native_mode else "standard"
    print(f"Running coverage analysis ({mode_label} mode)...")
    lcov_path = Path("lcov.info")

    # Clean previous profdata
    print("  Cleaning previous coverage data...")
    result = subprocess.run(
        ["cargo", "llvm-cov", "clean", "--workspace"],
        capture_output=True,
        text=True,
    )
    if result.returncode != 0:
        if result.stderr:
            print(result.stderr, file=sys.stderr)
        print("ERROR: Failed to clean coverage data", file=sys.stderr)
        sys.exit(1)

    # Phase 1: Run tests and collect profraw data (no report yet)
    print("  Running tests with coverage...")
    if native_mode:
        features_args = ["--features", "native"]
    else:
        features_args = ["--features", "rand,antithesis"]
    result = subprocess.run(
        ["cargo", "llvm-cov", "--no-report"] + features_args,
        capture_output=True,
        text=True,
    )
    if result.stdout:
        print(result.stdout)
    if result.returncode != 0:
        if result.stderr:
            print(result.stderr, file=sys.stderr)
        print("ERROR: Coverage run failed", file=sys.stderr)
        sys.exit(1)
    print("  Tests passed")

    # Phase 2: Generate LCOV report.
    # First try with subprocess binaries included (for TempRustProject coverage).
    # If that fails, fall back to standard report.
    llvm_cov_target = Path("target/llvm-cov-target")
    subprocess_bins = sorted(
        p
        for p in llvm_cov_target.glob("debug/temp_hegel_test_*")
        if p.is_file() and not p.suffix  # exclude .d, .pdb etc
    )
    print(f"  Generating report ({len(subprocess_bins)} subprocess binaries)...")

    if subprocess_bins:
        # Use raw llvm tools to include subprocess binaries.
        # Find llvm tools from the Rust toolchain.
        toolchain_result = subprocess.run(
            ["rustc", "--print", "sysroot"],
            capture_output=True,
            text=True,
        )
        sysroot = toolchain_result.stdout.strip()
        llvm_bin = Path(sysroot) / "lib/rustlib" / get_target_triple() / "bin"
        llvm_profdata = llvm_bin / "llvm-profdata"
        llvm_cov_bin = llvm_bin / "llvm-cov"

        if llvm_profdata.exists() and llvm_cov_bin.exists():
            # Merge all profraw files
            profraw_files = list(llvm_cov_target.glob("*.profraw"))
            merged_profdata = llvm_cov_target / "merged.profdata"
            result = subprocess.run(
                [str(llvm_profdata), "merge", "-sparse"]
                + [str(f) for f in profraw_files]
                + ["-o", str(merged_profdata)],
                capture_output=True,
                text=True,
            )
            if result.returncode != 0:
                print(
                    "WARNING: profdata merge failed, using standard report",
                    file=sys.stderr,
                )
            else:
                # Find all instrumented binaries (main test binaries + subprocess binaries)
                main_bins = sorted(
                    p
                    for p in llvm_cov_target.glob("debug/deps/hegel-*")
                    if p.is_file() and not p.suffix
                )
                all_bins = main_bins + subprocess_bins
                # Also include integration test binaries
                test_bins = sorted(
                    p
                    for p in llvm_cov_target.glob("debug/deps/test_*")
                    if p.is_file() and not p.suffix
                )
                all_bins.extend(test_bins)

                if all_bins:
                    # Generate LCOV with all objects
                    cmd = [
                        str(llvm_cov_bin),
                        "export",
                        "-format=lcov",
                        f"-instr-profile={merged_profdata}",
                    ]
                    # First binary is positional, rest are --object
                    cmd.append(str(all_bins[0]))
                    for b in all_bins[1:]:
                        cmd.extend(["-object", str(b)])
                    # Only report on project source files
                    cmd.extend(
                        [
                            "-ignore-filename-regex=\\.cargo/registry",
                            "-ignore-filename-regex=/rustc/",
                            "-ignore-filename-regex=/rustlib/",
                            "-ignore-filename-regex=/tmp/",
                            "-ignore-filename-regex=/var/folders/",
                            "-ignore-filename-regex=tests/",
                        ]
                    )

                    result = subprocess.run(cmd, capture_output=True, text=True)
                    if result.returncode == 0 and result.stdout:
                        lcov_path.write_text(result.stdout)
                        return lcov_path
                    else:
                        print(
                            "WARNING: llvm-cov export failed, using standard report",
                            file=sys.stderr,
                        )
                        if result.stderr:
                            print(result.stderr[:500], file=sys.stderr)

    # Fallback: standard cargo llvm-cov report
    result = subprocess.run(
        ["cargo", "llvm-cov", "report", "--lcov", f"--output-path={lcov_path}"],
        capture_output=True,
        text=True,
    )
    if result.returncode != 0:
        if result.stderr:
            print(result.stderr, file=sys.stderr)
        print("ERROR: Coverage report generation failed", file=sys.stderr)
        sys.exit(1)

    if not lcov_path.exists():
        print("ERROR: lcov.info was not generated", file=sys.stderr)
        sys.exit(1)

    return lcov_path


# ──────────────────────────────────────────────────────────────────────
# LCOV parsing
# ──────────────────────────────────────────────────────────────────────


def get_line_content(file_path: Path, line_number: int) -> str:
    """Get the content of a specific line from a file."""
    try:
        with file_path.open() as f:
            for i, line in enumerate(f, 1):
                if i == line_number:
                    return line.rstrip("\n")
    except (OSError, IOError):
        pass
    return ""


def parse_lcov(lcov_path: Path) -> CoverageData:
    """
    Parse lcov.info file to find uncovered lines.

    LCOV format:
        SF:<source file path>
        DA:<line number>,<execution count>
        ...
        end_of_record
    """
    data = CoverageData()
    current_file: Path | None = None

    with lcov_path.open() as f:
        for line in f:
            line = line.strip()

            if line.startswith("SF:"):
                current_file = Path(line[3:])

            elif line.startswith("DA:") and current_file is not None:
                match = re.match(r"DA:(\d+),(\d+)", line)
                if match:
                    line_number = int(match.group(1))
                    exec_count = int(match.group(2))

                    if exec_count == 0:
                        content = get_line_content(current_file, line_number)
                        data.uncovered_lines.append(
                            UncoveredLine(current_file, line_number, content)
                        )
                    else:
                        data.covered_lines.setdefault(current_file, set()).add(
                            line_number
                        )

            elif line == "end_of_record":
                current_file = None

    return data


# ──────────────────────────────────────────────────────────────────────
# Annotation management
# ──────────────────────────────────────────────────────────────────────


def find_line_annotations() -> list[tuple[Path, int, str]]:
    """Find all line-level occurrences of // nocov in source files.

    Returns (file, line_number, line_content) tuples.
    Does NOT match block markers (// nocov start / // nocov end).
    """
    pattern = re.compile(r"//\s*nocov\b")
    block_marker = re.compile(r"//\s*nocov\s+(start|end)\b")

    results: list[tuple[Path, int, str]] = []
    for src_dir in SOURCE_DIRS:
        if not src_dir.exists():
            continue
        for rs_file in sorted(src_dir.rglob("*.rs")):
            try:
                with rs_file.open() as f:
                    for i, line in enumerate(f, 1):
                        if pattern.search(line):
                            if block_marker.search(line):
                                continue
                            results.append((rs_file, i, line))
            except (OSError, IOError):
                continue
    return results


def remove_annotation_from_line(line: str) -> str:
    """Remove // nocov from the end of a source line."""
    pattern = r"\s*//\s*nocov\b.*$"
    cleaned = re.sub(pattern, "", line.rstrip("\n"))
    return cleaned + "\n" if line.endswith("\n") else cleaned


def cleanup_unnecessary_annotations(coverage: CoverageData) -> int:
    """Remove // nocov from covered lines.

    Only removes line-level annotations; block markers (// nocov start/end) are
    never auto-removed.

    Returns the number of annotations removed.
    """
    nocov_removed = 0

    # Collect modifications: {file: set of line numbers to clean}
    modifications: dict[Path, set[int]] = {}

    for file_path, line_num, _ in find_line_annotations():
        covered = coverage.covered_lines.get(file_path, set())
        if line_num in covered:
            modifications.setdefault(file_path, set()).add(line_num)
            nocov_removed += 1

    # Apply modifications
    for file_path, line_nums in modifications.items():
        try:
            with file_path.open() as f:
                lines = f.readlines()
            for line_num in line_nums:
                idx = line_num - 1
                if idx < len(lines):
                    lines[idx] = remove_annotation_from_line(lines[idx])
            with file_path.open("w") as f:
                f.writelines(lines)
        except (OSError, IOError) as e:
            print(f"WARNING: Could not modify {file_path}: {e}", file=sys.stderr)

    return nocov_removed


def count_annotations() -> int:
    """Count coverage annotations in source code.

    Returns the total number of nocov-excluded lines: line-level // nocov
    annotations plus lines inside // nocov start ... // nocov end blocks.
    """
    nocov_inline_pattern = re.compile(r"//\s*nocov\b")
    nocov_start_pattern = re.compile(r"//\s*nocov\s+start\b")
    nocov_end_pattern = re.compile(r"//\s*nocov\s+end\b")

    nocov_count = 0

    for src_dir in SOURCE_DIRS:
        if not src_dir.exists():
            continue
        for rs_file in sorted(src_dir.rglob("*.rs")):
            try:
                in_nocov_block = False
                with rs_file.open() as f:
                    for line in f:
                        if nocov_start_pattern.search(line):
                            in_nocov_block = True
                            continue
                        if nocov_end_pattern.search(line):
                            in_nocov_block = False
                            continue
                        if in_nocov_block:
                            # Count each line inside a block
                            nocov_count += 1
                        elif nocov_inline_pattern.search(line):
                            # Count inline // nocov annotations
                            nocov_count += 1
            except (OSError, IOError):
                continue

    return nocov_count


# ──────────────────────────────────────────────────────────────────────
# Ratchet
# ──────────────────────────────────────────────────────────────────────


def read_ratchet(key: str = "nocov") -> int | float:
    """Read the ratchet file. Returns the nocov limit for the given key.

    If the file doesn't exist or key is absent, returns inf to allow initialization.
    """
    if not RATCHET_FILE.exists():
        return float("inf")

    try:
        with RATCHET_FILE.open() as f:
            data = json.load(f)
        return data.get(key, float("inf"))
    except (json.JSONDecodeError, OSError, IOError):
        return float("inf")


def write_ratchet(nocov: int, key: str = "nocov") -> None:
    """Write the ratchet file, updating only the given key."""
    RATCHET_FILE.parent.mkdir(parents=True, exist_ok=True)
    # Read existing data so other keys are preserved.
    data: dict = {}
    if RATCHET_FILE.exists():
        try:
            with RATCHET_FILE.open() as f:
                data = json.load(f)
        except (json.JSONDecodeError, OSError, IOError):
            data = {}
    data[key] = nocov
    with RATCHET_FILE.open("w") as f:
        json.dump(data, f, indent=2, sort_keys=True)
        f.write("\n")



# ──────────────────────────────────────────────────────────────────────
# Analysis
# ──────────────────────────────────────────────────────────────────────


def check_uncovered_lines(uncovered: list[UncoveredLine]) -> int:
    """Categorize and report uncovered lines. Returns 0 if OK, 1 if failures."""
    structural: list[UncoveredLine] = []
    test_code: list[UncoveredLine] = []
    placeholders: list[UncoveredLine] = []
    macro_continuations: list[UncoveredLine] = []
    nocov: list[UncoveredLine] = []
    ignored_tests: list[UncoveredLine] = []
    actual: list[UncoveredLine] = []

    for line in uncovered:
        if line.is_structural_syntax_only():
            structural.append(line)
        elif line.is_test_code():
            test_code.append(line)
        elif line.is_todo_placeholder() or line.is_unreachable_placeholder():
            placeholders.append(line)
        elif line.is_inside_excluded_macro():
            macro_continuations.append(line)
        elif line.has_nocov_annotation():
            nocov.append(line)
        elif line.is_ignored_test_body():
            ignored_tests.append(line)
        else:
            actual.append(line)

    print()
    print("Line Coverage Analysis")
    print("======================")
    print()
    print(f"Uncovered closing braces (allowed):           {len(structural)}")
    print(f"Uncovered #[cfg(test)] code (allowed):        {len(test_code)}")
    print(f"Uncovered todo!/unreachable! (allowed):        {len(placeholders)}")
    print(f"Uncovered macro continuations (allowed):      {len(macro_continuations)}")
    print(f"Uncovered #[ignore]d test bodies (allowed):   {len(ignored_tests)}")
    print(f"Uncovered // nocov lines (allowed):           {len(nocov)}")
    print(f"Uncovered code lines:                         {len(actual)}")
    print()

    if not actual:
        print("All uncovered lines are allowable patterns.")
        return 0

    print("Found uncovered CODE that requires tests:")
    print()
    for line in actual:
        try:
            rel_path = line.file.relative_to(Path.cwd())
        except ValueError:
            rel_path = line.file
        print(f"  {rel_path}:{line.line_number}: {line.content.strip()}")
    print()
    print("Add tests for the uncovered code, or if truly untestable,")
    print("add a // nocov annotation.")
    return 1


# ──────────────────────────────────────────────────────────────────────
# Main
# ──────────────────────────────────────────────────────────────────────


def main() -> int:
    """Main entry point."""
    parser = argparse.ArgumentParser(description="Check code coverage")
    parser.add_argument(
        "--native",
        action="store_true",
        help="Run coverage in native mode (--features native, separate ratchet)",
    )
    args = parser.parse_args()
    native_mode: bool = args.native
    ratchet_key = "nocov_native" if native_mode else "nocov"

    # 1. Generate coverage
    lcov_path = run_coverage(native_mode=native_mode)

    # 2. Parse coverage data
    coverage = parse_lcov(lcov_path)

    # 3. Check uncovered lines
    if coverage.uncovered_lines:
        result = check_uncovered_lines(coverage.uncovered_lines)
        if result != 0:
            return result
    else:
        print("\n100% line coverage -- no uncovered lines at all!")

    # 4. Cleanup: remove annotations from code that is now covered
    nocov_removed = cleanup_unnecessary_annotations(coverage)
    if nocov_removed > 0:
        print(f"\nRemoved {nocov_removed} unnecessary // nocov annotations")

    # 5. Count remaining annotations
    nocov_count = count_annotations()
    print(f"\nCoverage annotations ({ratchet_key}): {nocov_count} // nocov")

    # 6. Check ratchet
    nocov_limit = read_ratchet(key=ratchet_key)

    if nocov_count > nocov_limit:
        print(f"\nCoverage annotation ratchet EXCEEDED!")
        print(f"  // nocov: {nocov_count} (limit: {nocov_limit})")
        print()
        print("The nocov ratchet may not be increased. Remove the")
        print("annotations or add tests to cover the code.")
        return 1

    ratchet_changed = False
    if nocov_count < nocov_limit:
        old = nocov_limit if nocov_limit != float("inf") else "none"
        print(f"  Ratchet tightened: {ratchet_key} {old} -> {nocov_count}")
        write_ratchet(nocov_count, key=ratchet_key)
        ratchet_changed = True
    elif not RATCHET_FILE.exists():
        write_ratchet(nocov_count, key=ratchet_key)
        ratchet_changed = True

    if ratchet_changed:
        print(f"  Updated {RATCHET_FILE}")

    print("\nCoverage check PASSED!")
    return 0


if __name__ == "__main__":
    sys.exit(main())
