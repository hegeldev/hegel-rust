#!/usr/bin/env python3
"""Check that // nocov annotations follow style rules.

1. No 3+ consecutive inline // nocov lines (use // nocov start/end blocks).
2. No inline // nocov adjacent to a // nocov start or // nocov end marker
   (expand the block instead).
3. No single-line // nocov start/end blocks when the line could take an inline
   comment (use inline // nocov instead).
4. No // nocov end immediately followed by // nocov start (merge the blocks).
5. // nocov start and // nocov end must be on their own lines (no code).
6. No inline // nocov inside a // nocov start/end block (redundant).
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

SOURCE_DIRS = [Path("src"), Path("hegel-macros/src")]

nocov_inline_re = re.compile(r"//\s*nocov\b")
nocov_start_re = re.compile(r"//\s*nocov\s+start\b")
nocov_end_re = re.compile(r"//\s*nocov\s+end\b")
nocov_block_re = re.compile(r"//\s*nocov\s+(start|end)\b")
nocov_block_own_line_re = re.compile(r"^\s*//\s*nocov\s+(start|end)\b\s*$")


def is_inline_nocov(line: str) -> bool:
    return bool(nocov_inline_re.search(line)) and not bool(nocov_block_re.search(line))


def check() -> int:
    violations: list[str] = []

    for src_dir in SOURCE_DIRS:
        if not src_dir.exists():
            continue
        for rs_file in sorted(src_dir.rglob("*.rs")):
            try:
                lines = rs_file.read_text().splitlines()
            except (OSError, IOError):
                continue

            in_block = False
            block_start_line = -1
            block_content_lines = 0
            block_single_line_could_be_inline = False
            run_start = -1
            run_length = 0

            for i, line in enumerate(lines):
                lineno = i + 1

                if nocov_start_re.search(line):
                    # Check: start/end must be on their own line
                    if not nocov_block_own_line_re.match(line):
                        violations.append(
                            f"  {rs_file}:{lineno}: // nocov start must be on its own line"
                        )
                    # Check: inline nocov right before this start
                    if i > 0 and is_inline_nocov(lines[i - 1]):
                        violations.append(
                            f"  {rs_file}:{lineno - 1}: inline // nocov adjacent to // nocov start (expand the block)"
                        )
                    # Check: run ending at a block boundary
                    if run_length >= 3:
                        violations.append(
                            f"  {rs_file}:{run_start}: {run_length} consecutive inline // nocov (use a block)"
                        )
                    run_length = 0
                    in_block = True
                    block_start_line = lineno
                    block_content_lines = 0
                    block_single_line_could_be_inline = False
                    continue

                if nocov_end_re.search(line):
                    # Check: end must be on its own line
                    if not nocov_block_own_line_re.match(line):
                        violations.append(
                            f"  {rs_file}:{lineno}: // nocov end must be on its own line"
                        )
                    # Check: single-line block where the line could take inline // nocov.
                    # Lines ending with { or that are continuations of multi-line
                    # expressions can't take inline comments (cargo fmt moves them).
                    if block_content_lines == 1 and block_single_line_could_be_inline:
                        violations.append(
                            f"  {rs_file}:{block_start_line}: single-line // nocov start/end block (use inline // nocov instead)"
                        )
                    in_block = False
                    # Check: inline nocov right after this end
                    if i + 1 < len(lines) and is_inline_nocov(lines[i + 1]):
                        violations.append(
                            f"  {rs_file}:{lineno + 1}: inline // nocov adjacent to // nocov end (expand the block)"
                        )
                    # Check: // nocov start right after this end (merge the blocks)
                    if i + 1 < len(lines) and nocov_start_re.search(lines[i + 1]):
                        violations.append(
                            f"  {rs_file}:{lineno}: // nocov end immediately followed by // nocov start (merge the blocks)"
                        )
                    continue

                if in_block:
                    block_content_lines += 1
                    # A line can take inline // nocov if it ends with ; or ,
                    # (a complete statement or item). Lines ending with { or
                    # that are expression continuations get reformatted by
                    # cargo fmt if you add a trailing comment.
                    stripped_end = line.rstrip()
                    block_single_line_could_be_inline = stripped_end.endswith((";", ","))
                    # Check: no inline nocov inside a block
                    if is_inline_nocov(line):
                        violations.append(
                            f"  {rs_file}:{lineno}: inline // nocov inside a // nocov start/end block (redundant)"
                        )
                    continue

                if is_inline_nocov(line):
                    if run_length == 0:
                        run_start = lineno
                    run_length += 1
                else:
                    if run_length >= 3:
                        violations.append(
                            f"  {rs_file}:{run_start}: {run_length} consecutive inline // nocov (use a block)"
                        )
                    run_length = 0

            if run_length >= 3:
                violations.append(
                    f"  {rs_file}:{run_start}: {run_length} consecutive inline // nocov (use a block)"
                )

    if violations:
        print("nocov style violations found:\n")
        for v in violations:
            print(v)
        return 1

    return 0


if __name__ == "__main__":
    sys.exit(check())
