#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.10"
# ///
"""
Custom Code Coverage Check Script
=================================

PURPOSE:
This script provides a more nuanced coverage check than simple percentage thresholds.
LLVM's coverage instrumentation counts regions which sometimes includes closing
braces of control structures as separate coverage points. Additionally, certain
patterns like todo!() and unreachable!() placeholders are expected to be uncovered.

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

4. #[ignore]d test bodies
   LLVM-cov marks the body of #[ignore]d tests as uncovered because the
   test framework compiles them but never runs them.

SECURITY NOTE:
--------------
This script should only be modified to allow ADDITIONAL exclusions with EXTREME CAUTION.
Adding new patterns to the allowlist could mask actual coverage gaps.
"""

from __future__ import annotations

import re
import subprocess
import sys
from dataclasses import dataclass
from pathlib import Path


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
        # Match todo!() with optional message and optional semicolon
        return bool(re.match(r'^todo!\s*\([^)]*\)\s*;?\s*$', stripped))

    def is_unreachable_placeholder(self) -> bool:
        """
        Check if this line contains unreachable!() as the primary action.

        Matches standalone unreachable!() and match-arm unreachable!():
            unreachable!("msg")
            _ => unreachable!("msg"),
            unreachable!(            // multi-line start
        """
        stripped = self.content.strip()
        # Single-line: unreachable!("msg"); — use greedy match for nested parens
        if re.search(r'unreachable!\s*\(.*\)\s*[;,]?\s*$', stripped):
            return True
        # Multi-line start: unreachable!( without closing paren
        if re.search(r'unreachable!\s*\(', stripped) and not stripped.rstrip(';').rstrip().endswith(')'):
            return True
        return False

    def is_test_code(self) -> bool:
        """
        Check if this line is inside a #[cfg(test)] module.

        Test code coverage is not meaningful — we care about coverage of
        production code, not test helpers.

        Scans backward from this line looking for `#[cfg(test)]`.
        """
        try:
            with self.file.open() as f:
                lines = f.readlines()
        except (OSError, IOError):
            return False

        idx = self.line_number - 1  # 0-indexed
        # Track brace depth to determine if we're inside a #[cfg(test)] module
        # Walk backward counting braces to find the module boundary
        brace_depth = 0
        for i in range(idx, -1, -1):
            line = lines[i].strip() if i < len(lines) else ""
            brace_depth += line.count('}')
            brace_depth -= line.count('{')
            # If we've found the opening brace of our enclosing module,
            # check if it's a #[cfg(test)] module
            if brace_depth < 0:
                # Look at the lines just above this opening brace for #[cfg(test)]
                for j in range(i, max(i - 5, -1), -1):
                    check_line = lines[j].strip() if j < len(lines) else ""
                    if "#[cfg(test)]" in check_line:
                        return True
                # Not a test module — but we might be in a nested block
                # inside a test module, so keep scanning
                brace_depth = 0
        return False

    def is_ignored_test_body(self) -> bool:
        """
        Check if this line is inside an `#[ignore]`d test function.

        LLVM-cov marks the body of #[ignore]d tests as uncovered because
        the test framework compiles them but never runs them.  These are
        expected to be uncovered.

        Scans backward from this line looking for #[ignore, then confirms
        it is preceded by #[test] (within 3 lines).
        """
        try:
            with self.file.open() as f:
                lines = f.readlines()
        except (OSError, IOError):
            return False

        # Walk backward from this line looking for `fn ` (the test function start)
        idx = self.line_number - 1  # 0-indexed
        for i in range(idx, max(idx - 50, -1), -1):
            line = lines[i].strip() if i < len(lines) else ""
            if line.startswith("fn "):
                # Found the enclosing function. Now check if #[ignore is above it.
                for j in range(i - 1, max(i - 4, -1), -1):
                    attr = lines[j].strip() if j < len(lines) else ""
                    if attr.startswith("#[ignore"):
                        return True
                return False
        return False


def get_target_triple() -> str:
    """Get the current Rust target triple."""
    result = subprocess.run(
        ["rustc", "-vV"], capture_output=True, text=True,
    )
    for line in result.stdout.splitlines():
        if line.startswith("host:"):
            return line.split(":")[1].strip()
    return "unknown"


def run_coverage() -> Path:
    """Run coverage analysis and generate LCOV report."""
    print("Running coverage analysis...")
    lcov_path = Path("lcov.info")

    # Clean previous profdata
    print("  Cleaning previous coverage data...")
    result = subprocess.run(
        ["cargo", "llvm-cov", "clean", "--workspace"],
        capture_output=True, text=True,
    )
    if result.returncode != 0:
        if result.stderr:
            print(result.stderr, file=sys.stderr)
        print("ERROR: Failed to clean coverage data", file=sys.stderr)
        sys.exit(1)

    # Phase 1: Run tests and collect profraw data (no report yet)
    print("  Running tests with coverage...")
    result = subprocess.run(
        ["cargo", "llvm-cov", "--no-report", "--all-features"],
        capture_output=True, text=True,
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
        p for p in llvm_cov_target.glob("debug/temp_hegel_test_*")
        if p.is_file() and not p.suffix  # exclude .d, .pdb etc
    )
    print(f"  Generating report ({len(subprocess_bins)} subprocess binaries)...")

    if subprocess_bins:
        # Use raw llvm tools to include subprocess binaries.
        # Find llvm tools from the Rust toolchain.
        toolchain_result = subprocess.run(
            ["rustc", "--print", "sysroot"], capture_output=True, text=True,
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
                capture_output=True, text=True,
            )
            if result.returncode != 0:
                print(f"WARNING: profdata merge failed, using standard report", file=sys.stderr)
            else:
                # Find all instrumented binaries (main test binaries + subprocess binaries)
                main_bins = sorted(
                    p for p in llvm_cov_target.glob("debug/deps/hegel-*")
                    if p.is_file() and not p.suffix
                )
                all_bins = main_bins + subprocess_bins
                # Also include integration test binaries
                test_bins = sorted(
                    p for p in llvm_cov_target.glob("debug/deps/test_*")
                    if p.is_file() and not p.suffix
                )
                all_bins.extend(test_bins)

                if all_bins:
                    # Generate LCOV with all objects
                    cmd = [
                        str(llvm_cov_bin), "export", "-format=lcov",
                        f"-instr-profile={merged_profdata}",
                    ]
                    # First binary is positional, rest are --object
                    cmd.append(str(all_bins[0]))
                    for b in all_bins[1:]:
                        cmd.extend(["-object", str(b)])
                    # Only report on project source files
                    cmd.extend([
                        "-ignore-filename-regex=\\.cargo/registry",
                        "-ignore-filename-regex=/rustc/",
                        "-ignore-filename-regex=/rustlib/",
                        "-ignore-filename-regex=/tmp/",
                        "-ignore-filename-regex=/var/folders/",
                        "-ignore-filename-regex=tests/",
                    ])

                    result = subprocess.run(cmd, capture_output=True, text=True)
                    if result.returncode == 0 and result.stdout:
                        lcov_path.write_text(result.stdout)
                        return lcov_path
                    else:
                        print(f"WARNING: llvm-cov export failed, using standard report", file=sys.stderr)
                        if result.stderr:
                            print(result.stderr[:500], file=sys.stderr)

    # Fallback: standard cargo llvm-cov report
    result = subprocess.run(
        ["cargo", "llvm-cov", "report", "--lcov", f"--output-path={lcov_path}"],
        capture_output=True, text=True,
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


def parse_lcov(lcov_path: Path) -> list[UncoveredLine]:
    """
    Parse lcov.info file to find uncovered lines.

    LCOV format:
        SF:<source file path>
        DA:<line number>,<execution count>
        ...
        end_of_record
    """
    uncovered: list[UncoveredLine] = []
    current_file: Path | None = None

    with lcov_path.open() as f:
        for line in f:
            line = line.strip()

            if line.startswith("SF:"):
                current_file = Path(line[3:])

            elif line.startswith("DA:") and current_file is not None:
                # DA:line_number,execution_count
                match = re.match(r"DA:(\d+),(\d+)", line)
                if match:
                    line_number = int(match.group(1))
                    exec_count = int(match.group(2))

                    if exec_count == 0:
                        # Get the actual content of the line
                        content = get_line_content(current_file, line_number)
                        uncovered.append(
                            UncoveredLine(
                                file=current_file,
                                line_number=line_number,
                                content=content,
                            )
                        )

            elif line == "end_of_record":
                current_file = None

    return uncovered


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


def main() -> int:
    """Main entry point."""
    # Generate coverage
    lcov_path = run_coverage()

    # Parse uncovered lines
    uncovered = parse_lcov(lcov_path)

    if not uncovered:
        print("\n\u2713 100% line coverage achieved!")
        return 0

    # Categorize uncovered lines
    structural_only: list[UncoveredLine] = []
    todo_placeholders: list[UncoveredLine] = []
    test_code_lines: list[UncoveredLine] = []
    ignored_test_lines: list[UncoveredLine] = []
    actual_code: list[UncoveredLine] = []

    for line in uncovered:
        if line.is_structural_syntax_only():
            structural_only.append(line)
        elif line.is_test_code():
            test_code_lines.append(line)
        elif line.is_todo_placeholder() or line.is_unreachable_placeholder():
            todo_placeholders.append(line)
        elif line.is_ignored_test_body():
            ignored_test_lines.append(line)
        else:
            actual_code.append(line)

    # Report results
    print()
    print("Coverage Analysis Results")
    print("=========================")
    print()
    print(f"Uncovered closing braces (allowed): {len(structural_only)}")
    print(f"Uncovered #[cfg(test)] code (allowed): {len(test_code_lines)}")
    print(f"Uncovered todo!/unreachable! (allowed): {len(todo_placeholders)}")
    print(f"Uncovered #[ignore]d test bodies (allowed): {len(ignored_test_lines)}")
    print(f"Uncovered code lines: {len(actual_code)}")
    print()

    if not actual_code:
        print("\u2713 All uncovered lines are allowable patterns.")
        print("  Coverage check PASSED!")
        return 0
    else:
        print("\u2717 Found uncovered CODE that requires tests:")
        print()
        for line in actual_code:
            # Show relative path for readability
            try:
                rel_path = line.file.relative_to(Path.cwd())
            except ValueError:
                rel_path = line.file
            print(f"  {rel_path}:{line.line_number}: {line.content.strip()}")
        print()
        print("ACTION REQUIRED:")
        print("  1. Add tests for the uncovered code, OR")
        print("  2. If truly untestable, document why and update this script")
        print()
        print("DO NOT blindly add exceptions! Each exclusion must be justified.")
        return 1


if __name__ == "__main__":
    sys.exit(main())
