#!/usr/bin/env python3
"""Generate `src/ffi/sys/fns.rs`, the `for_each_hegel_fn!` list of every
`hegel_*` function libhegel exports, from `hegel-c/src/lib.rs`.

The frontend's runtime loader resolves each listed function out of the
`libhegel_c` shared library, and the `static-engine` drift test
compile-asserts each listed signature against the real `hegel_c`
definition. A hand-maintained list can only be checked against what it
lists: a function added to the engine and called from `src/ffi.rs` but
missing here compiles fine under `static-engine` and fails only in a
default-features build. Deriving the list from the engine source closes
that gap, the same way cbindgen derives `hegel-c/include/hegel.h`.

The signatures are already Rust, so this is text extraction: every
`#[unsafe(no_mangle)] pub [unsafe] extern "C" fn hegel_*` at the top level
of `lib.rs`, with its parameter list and return type collapsed onto one
line, sorted by name.

Usage:
    scripts/gen-ffi-list.py           # rewrite src/ffi/sys/fns.rs
    scripts/gen-ffi-list.py --check   # exit 1 if it is out of date
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

ENGINE_SOURCE = Path("hegel-c/src/lib.rs")
OUTPUT = Path("src/ffi/sys/fns.rs")
REFRESH_COMMAND = "just c-header"

FN_RE = re.compile(
    r'^(?P<attr>#\[(?:unsafe\()?no_mangle\)?\]\n)?'
    r'pub (?:unsafe )?extern "C" fn (?P<name>hegel_\w+)\(',
    re.MULTILINE,
)

HEADER = f"""\
//! The `for_each_hegel_fn!` list: every `hegel_*` function libhegel exports,
//! with the signature the engine declares it with.
//!
//! Generated from `{ENGINE_SOURCE}` by `scripts/gen-ffi-list.py`. Do not
//! edit; run `{REFRESH_COMMAND}` after changing the engine's exports.
"""


def matching_paren(text: str, open_index: int) -> int:
    depth = 0
    for i in range(open_index, len(text)):
        if text[i] == "(":
            depth += 1
        elif text[i] == ")":
            depth -= 1
            if depth == 0:
                return i
    raise ValueError(f"unbalanced parentheses at offset {open_index}")


def collapse(text: str) -> str:
    return re.sub(r"\s+", " ", text).strip().rstrip(",").strip()


def extract_signatures(source: str) -> list[str]:
    signatures: dict[str, str] = {}
    for m in FN_RE.finditer(source):
        name = m.group("name")
        if not m.group("attr"):
            raise ValueError(f"{name} is `pub extern \"C\"` but not `#[unsafe(no_mangle)]`")
        open_index = m.end() - 1
        close_index = matching_paren(source, open_index)
        params = collapse(source[open_index + 1 : close_index])
        body_start = source.index("{", close_index)
        tail = source[close_index + 1 : body_start].strip()
        if tail.startswith("->"):
            ret = collapse(tail[2:])
        elif tail == "":
            ret = "()"
        else:
            raise ValueError(f"{name}: unexpected text between parameters and body: {tail!r}")
        if name in signatures:
            raise ValueError(f"{name} is defined twice")
        signatures[name] = f"fn {name}({params}) -> {ret};"
    return [signatures[name] for name in sorted(signatures)]


def render(signatures: list[str]) -> str:
    lines = [HEADER, "macro_rules! for_each_hegel_fn {", "    ($callback:ident) => {", "        $callback! {"]
    lines.extend(f"            {sig}" for sig in signatures)
    lines.extend(["        }", "    };", "}", "pub(crate) use for_each_hegel_fn;", ""])
    return "\n".join(lines)


def main(argv: list[str]) -> int:
    check = argv == ["--check"]
    if argv and not check:
        print(__doc__, file=sys.stderr)
        return 2
    expected = render(extract_signatures(ENGINE_SOURCE.read_text()))
    if check:
        actual = OUTPUT.read_text() if OUTPUT.exists() else ""
        if actual != expected:
            print(f"{OUTPUT} is out of date. Run `{REFRESH_COMMAND}` to refresh it.")
            return 1
        return 0
    OUTPUT.write_text(expected)
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
