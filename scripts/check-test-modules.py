#!/usr/bin/env python3
"""Check that every test .rs file is actually wired into a compiled target.

Cargo's integration-test discovery only looks at `tests/*.rs` and
`tests/*/main.rs`; everything deeper is only compiled when something
references it. Without that reference, the file is silently skipped — no
warning, no error, no failing test. This script catches such orphans in
both crates' test trees:

- `tests/<lib>/**` files must be reachable from `tests/<lib>/main.rs` via a
  chain of `mod <name>;` declarations (nested directories included).
- `tests/common/**` (shared helper modules) likewise from their `mod.rs`.
- `tests/embedded/**` files have no main.rs: each must be the target of a
  `#[cfg(test)] #[path = "…"]` include somewhere in the crate's `src/`.
- Files that are explicit cargo targets (e.g. the `[[bin]]` fixtures under
  `tests/fixtures/`, declared with `path = "tests/…"` in the crate's
  Cargo.toml) are wired by cargo itself.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

# Each test root, paired with the src trees whose `#[path]` attributes wire
# up its `embedded/` directory.
ROOTS: list[tuple[Path, list[Path]]] = [
    (Path("tests"), [Path("src")]),
    (Path("hegel-c/tests"), [Path("hegel-c/src")]),
    (Path("hegel-macros/tests"), [Path("hegel-macros/src")]),
]

MOD_RE = re.compile(r"^\s*(?:pub\s+)?mod\s+(\w+)\s*;", re.MULTILINE)
PATH_ATTR_RE = re.compile(r'#\[path\s*=\s*"([^"]+)"\s*\]')
INCLUDE_RE = re.compile(r'include!\s*\(\s*"([^"]+)"\s*\)')
TARGET_PATH_RE = re.compile(r'^path\s*=\s*"([^"]+)"\s*$', re.MULTILINE)
EMBEDDED_MARKER = "tests/embedded/"


def cargo_target_files(tests_root: Path) -> set[Path]:
    """Files under `tests_root` that the crate's Cargo.toml wires up as
    explicit targets (`path = "tests/…"` in a [[bin]]/[[test]]/[[bench]]
    section) — cargo compiles these directly."""
    manifest = tests_root.parent / "Cargo.toml"
    if not manifest.exists():
        return set()
    targets: set[Path] = set()
    for p in TARGET_PATH_RE.findall(manifest.read_text()):
        candidate = (tests_root.parent / p).resolve()
        if candidate.is_relative_to(tests_root.resolve()):
            targets.add(candidate)
    return targets


def embedded_targets(src_roots: list[Path]) -> set[str]:
    """Every `tests/embedded/`-relative path referenced by a `#[path]`
    attribute in the given src trees (or by another embedded file)."""
    targets: set[str] = set()
    scan_roots = list(src_roots)
    for src_root in scan_roots:
        if not src_root.is_dir():
            continue
        for f in src_root.rglob("*.rs"):
            for p in PATH_ATTR_RE.findall(f.read_text()):
                idx = p.find(EMBEDDED_MARKER)
                if idx != -1:
                    targets.add(p[idx + len(EMBEDDED_MARKER) :])
    return targets


def check_module_tree(directory: Path, module_file: Path, violations: list[str]) -> None:
    """Require every .rs file/subdirectory under `directory` to be declared
    with `mod <name>;` in `module_file`, recursing into subdirectories."""
    declared = set(MOD_RE.findall(module_file.read_text()))

    for entry in sorted(directory.iterdir()):
        if entry.is_file() and entry.suffix == ".rs":
            if entry == module_file or entry.name in ("main.rs", "mod.rs"):
                continue
            if entry.stem not in declared:
                violations.append(
                    f"  {entry}: not referenced from {module_file} "
                    f"(add `mod {entry.stem};` to {module_file})"
                )
        elif entry.is_dir():
            sibling = directory / f"{entry.name}.rs"
            nested = entry / "mod.rs"
            if entry.name not in declared:
                violations.append(
                    f"  {entry}/: not referenced from {module_file} "
                    f"(add `mod {entry.name};` to {module_file})"
                )
            if sibling.exists():
                check_module_tree(entry, sibling, violations)
            elif nested.exists():
                check_module_tree(entry, nested, violations)
            else:
                violations.append(
                    f"  {entry}/: has no {entry.name}.rs or mod.rs module file, "
                    "so nothing inside it can be compiled"
                )


def check_embedded(directory: Path, src_roots: list[Path], violations: list[str]) -> None:
    targets = embedded_targets(src_roots + [directory])
    included: set[str] = set()
    for f in directory.rglob("*.rs"):
        for inc in INCLUDE_RE.findall(f.read_text()):
            target = (f.parent / inc).resolve()
            if target.is_relative_to(directory.resolve()):
                included.add(target.relative_to(directory.resolve()).as_posix())
    for f in sorted(directory.rglob("*.rs")):
        rel = f.relative_to(directory).as_posix()
        if rel not in targets and rel not in included:
            violations.append(
                f"  {f}: not the target of any `#[path = \"…{EMBEDDED_MARKER}{rel}\"]` "
                f"or `include!` reference in {'/'.join(str(r) for r in src_roots)}"
            )


def check() -> int:
    violations: list[str] = []

    for tests_root, src_roots in ROOTS:
        if not tests_root.is_dir():
            continue
        target_files = cargo_target_files(tests_root)
        for lib_dir in sorted(tests_root.iterdir()):
            if not lib_dir.is_dir():
                continue
            if lib_dir.name == "embedded":
                check_embedded(lib_dir, src_roots, violations)
                continue
            if lib_dir.name == "ui" or lib_dir.name.startswith("ui-"):
                # trybuild UI cases: the driver's `compile_fail("tests/<dir>/*.rs")`
                # glob compiles every file, so none can be orphaned — but the
                # driver itself must exist and reference the glob.
                driver = tests_root / "test_ui.rs"
                needle = f"tests/{lib_dir.name}/"
                if not (driver.exists() and needle in driver.read_text()):
                    violations.append(
                        f"  {lib_dir}/: expected {driver} to reference "
                        f'"{needle}" via trybuild'
                    )
                continue
            main_rs = lib_dir / "main.rs"
            mod_rs = lib_dir / "mod.rs"
            if main_rs.exists():
                check_module_tree(lib_dir, main_rs, violations)
            elif mod_rs.exists():
                check_module_tree(lib_dir, mod_rs, violations)
            elif any(f.resolve() in target_files for f in lib_dir.rglob("*.rs")):
                # A directory of explicit cargo targets (fixture binaries):
                # every file must itself be a target declared in Cargo.toml.
                for f in sorted(lib_dir.rglob("*.rs")):
                    if f.resolve() not in target_files:
                        violations.append(
                            f"  {f}: not declared as a cargo target in "
                            f"{tests_root.parent / 'Cargo.toml'} (add a [[bin]] "
                            "entry, or wire it into a module tree)"
                        )
            else:
                violations.append(
                    f"  {lib_dir}/: has no main.rs (integration-test target) or "
                    "mod.rs (shared helper module), so nothing inside it is compiled"
                )

    if violations:
        print("Orphan test files found:\n")
        for v in violations:
            print(v)
        print(
            "\nTest files are only compiled if something references them: a"
            " `mod <name>;` chain from main.rs/mod.rs, or a `#[path]` include"
            " from src/ for embedded tests. Add the reference, or delete the"
            " file."
        )
        return 1

    return 0


if __name__ == "__main__":
    sys.exit(check())
