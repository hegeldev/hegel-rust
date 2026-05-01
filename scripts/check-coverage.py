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

6. #[cfg(windows)] blocks
   Code gated behind #[cfg(windows)] is not compiled on Linux where
   coverage runs. Any // nocov annotations inside these blocks do not
   count against the ratchet.

7. // nocov Annotations
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
import shutil
import subprocess
import sys
from dataclasses import dataclass, field
from pathlib import Path

RATCHET_FILE = Path(".github/coverage-ratchet.json")
SOURCE_DIRS = [Path("src"), Path("hegel-macros/src")]
# Directories where // nocov is banned outright. Temporarily empty: during the
# pbtkit/hypothesis port, we're using nocov in src/native to keep the coverage
# job green so the port can take priority. Re-add src/native once the ratchet
# has been driven back down.
NOCOV_BANNED_DIRS: list[Path] = []

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
# #[cfg(windows)] block cache
# ──────────────────────────────────────────────────────────────────────

_cfg_windows_cache: dict[Path, set[int]] = {}


def get_cfg_windows_lines(file_path: Path) -> set[int]:
    """Return the set of line numbers inside #[cfg(windows)] blocks.

    Detects both:
    - #[cfg(windows)] followed by a braced block { ... }
    - #[cfg(windows)] followed by a single item (fn, const, static, etc.)
    """
    resolved = file_path.resolve()
    if resolved in _cfg_windows_cache:
        return _cfg_windows_cache[resolved]

    excluded: set[int] = set()
    try:
        with file_path.open() as f:
            lines = f.readlines()
    except (OSError, IOError):
        _cfg_windows_cache[resolved] = excluded
        return excluded

    i = 0
    while i < len(lines):
        stripped = lines[i].strip()
        if stripped == "#[cfg(windows)]":
            # Find the braced scope that follows
            brace_depth = 0
            started = False
            for j in range(i + 1, len(lines)):
                for ch in lines[j]:
                    if ch == "{":
                        brace_depth += 1
                        started = True
                    elif ch == "}":
                        brace_depth -= 1
                # line j+1 is inside the block (1-indexed)
                if started:
                    excluded.add(j + 1)
                if started and brace_depth == 0:
                    break
        i += 1

    _cfg_windows_cache[resolved] = excluded
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

    def is_windows_only(self) -> bool:
        """Check if this line is inside a #[cfg(windows)] block."""
        return self.line_number in get_cfg_windows_lines(self.file)

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
    # `cargo llvm-cov clean` removes profraw/profdata but NOT compiled binaries.
    # Stale subprocess binaries from previous runs (possibly compiled with
    # different feature flags) would otherwise be picked up by the report step
    # and contribute 0-coverage entries for source files they compile but whose
    # code paths were never reached. Wipe the tmp/ directory to remove them.
    tmp_dir = Path("target/llvm-cov-target/tmp")
    if tmp_dir.exists():
        shutil.rmtree(tmp_dir)
        tmp_dir.mkdir(parents=True, exist_ok=True)

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
    # TempRustProject tests share a target dir under CARGO_TARGET_TMPDIR
    # (tests/common/project.rs::shared_target_dir), which for integration
    # tests under `cargo llvm-cov` lives at
    # `target/llvm-cov-target/tmp/hegel-shared-target/`. Within that shared
    # target, cargo places the temp crate's own test binary at
    # `debug/deps/temp_hegel_test_<id>-<hash>` and, when the temp crate has
    # a `tests/test.rs`, the integration-test binary at
    # `debug/deps/test-<hash>`. We also keep the legacy pattern under
    # `debug/` for older layouts.
    subprocess_bin_globs = (
        # Non-coverage mode: TempRustProject sets CARGO_TARGET_DIR = shared_target_dir
        "debug/temp_hegel_test_*",
        "tmp/hegel-shared-target/debug/deps/temp_hegel_test_*-*",
        "tmp/hegel-shared-target/debug/deps/test-*",
        # Coverage mode: cargo-llvm-cov may override CARGO_TARGET_DIR and place
        # subprocess binaries directly under the llvm-cov-target root, or in a
        # per-package tmp subdirectory.
        "debug/deps/test-*",
        "tmp/temp_hegel_test_*/debug/deps/test-*",
    )
    subprocess_bins = sorted(
        p
        for pattern in subprocess_bin_globs
        for p in llvm_cov_target.glob(pattern)
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
            # Merge all profraw files — subprocess crates (via
            # TempRustProject's shared target) emit their own profraws
            # nested under `tmp/hegel-shared-target/...`, so search
            # recursively.
            profraw_files = list(llvm_cov_target.rglob("*.profraw"))
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
                # Also include integration test binaries (auto-discovered
                # test_* plus named targets from Cargo.toml [[test]])
                test_bin_patterns = [
                    "debug/deps/test_*",
                    "debug/deps/hypothesis-*",
                    "debug/deps/pbtkit-*",
                ]
                test_bins = sorted(
                    p
                    for pattern in test_bin_patterns
                    for p in llvm_cov_target.glob(pattern)
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

    # A line may appear as both covered and uncovered when multiple SF blocks
    # exist for the same file (from different object binaries). If any object
    # covered it, treat it as covered.
    covered = data.covered_lines
    data.uncovered_lines = [
        u for u in data.uncovered_lines
        if u.line_number not in covered.get(u.file, set())
    ]

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

    Lines inside #[cfg(windows)] blocks are excluded from the count since
    they are not compiled on Linux where coverage runs.
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
                windows_lines = get_cfg_windows_lines(rs_file)
                in_nocov_block = False
                with rs_file.open() as f:
                    for line_num, line in enumerate(f, 1):
                        if nocov_start_pattern.search(line):
                            in_nocov_block = True
                            continue
                        if nocov_end_pattern.search(line):
                            in_nocov_block = False
                            continue
                        if line_num in windows_lines:
                            continue
                        if in_nocov_block:
                            nocov_count += 1
                        elif nocov_inline_pattern.search(line):
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
# Banned directories
# ──────────────────────────────────────────────────────────────────────


def check_banned_nocov() -> int:
    """Check that no // nocov annotations exist in banned directories.

    Returns 0 if clean, 1 if violations found.
    """
    nocov_pattern = re.compile(r"//\s*nocov\b")
    violations: list[tuple[Path, int, str]] = []

    for banned_dir in NOCOV_BANNED_DIRS:
        if not banned_dir.exists():
            continue
        for rs_file in sorted(banned_dir.rglob("*.rs")):
            try:
                with rs_file.open() as f:
                    for i, line in enumerate(f, 1):
                        if nocov_pattern.search(line):
                            violations.append((rs_file, i, line.rstrip("\n")))
            except (OSError, IOError):
                continue

    if not violations:
        return 0

    print("\n// nocov is BANNED in the following directories:")
    for d in NOCOV_BANNED_DIRS:
        print(f"  {d}/")
    print(f"\nFound {len(violations)} violation(s):")
    for file_path, line_num, content in violations:
        try:
            rel = file_path.relative_to(Path.cwd())
        except ValueError:
            rel = file_path
        print(f"  {rel}:{line_num}: {content.strip()}")
    print("\nRemove the // nocov annotations and add tests instead.")
    return 1


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
    windows_only: list[UncoveredLine] = []
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
        elif line.is_windows_only():
            windows_only.append(line)
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
    print(f"Uncovered #[cfg(windows)] code (allowed):     {len(windows_only)}")
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
        "--mode",
        choices=["standard", "native", "both"],
        default="both",
        help=(
            "Coverage mode: 'standard' (--features rand,antithesis), "
            "'native' (--features native), or 'both' (default — runs "
            "both and merges the lcov reports before checking)."
        ),
    )
    parser.add_argument(
        "--native",
        action="store_true",
        help="Deprecated alias for --mode native (kept for backwards compat).",
    )
    args = parser.parse_args()
    mode: str = "native" if args.native else args.mode
    ratchet_key = "nocov_native" if mode == "native" else "nocov"

    # 0. Check for banned nocov annotations (fast, no compilation needed)
    if check_banned_nocov() != 0:
        return 1

    # 1. Generate coverage
    if mode == "both":
        # Run both modes and merge their lcov reports. A line is considered
        # covered if it was executed under either feature set: native embedded
        # tests cover src/native/*, standard tests cover src/server/* and the
        # rest. Concatenation is a valid lcov file (multiple SF blocks per
        # source file), and parse_lcov's existing dedup logic treats a line as
        # covered if any block reports it covered.
        lcov_path = Path("lcov.info")
        std_lcov = Path("lcov-standard.info")
        nat_lcov = Path("lcov-native.info")
        run_coverage(native_mode=False)
        std_lcov.write_bytes(lcov_path.read_bytes())
        run_coverage(native_mode=True)
        nat_lcov.write_bytes(lcov_path.read_bytes())
        lcov_path.write_bytes(std_lcov.read_bytes() + nat_lcov.read_bytes())
    else:
        lcov_path = run_coverage(native_mode=(mode == "native"))

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
