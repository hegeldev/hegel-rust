#!/usr/bin/env python3
"""Tests for the version-rewriting half of release.py.

The publish/git/gh half of `release()` can only run in CI with real
credentials, but the file rewriting (`apply_version_bump`) is pure and is
exactly where the inversion refactor broke the release: the root crate
started depending on `hegeltest-c` via a `=`-pinned path dependency, and the
old hardcoded bump list never touched it, so `cargo update --workspace` blew
up with "failed to select a version for the requirement `hegeltest-c =
0.19.1`" after the package version had already moved to 0.19.2.
"""

from __future__ import annotations

import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import release

ROOT_CARGO = """\
[package]
name = "hegeltest"
version = "0.19.1"
edition = "2024"

[dependencies]
hegeltest-macros = { version = "=0.19.1", path = "hegel-macros" }
hegeltest-c = { version = "=0.19.1", path = "hegel-c", default-features = false }
serde = { version = "1.0.103", features = ["derive"] }

[features]
__bench = ["hegeltest-c/__bench"]
"""

MACROS_CARGO = """\
[package]
name = "hegeltest-macros"
version = "0.19.1"
edition = "2024"

[dependencies]
syn = { version = "2.0", features = ["full"] }
"""

# Post-inversion: hegel-c no longer depends on hegeltest. It must keep
# bumping cleanly (its package version moves; it has no internal deps to
# touch) so that a future re-inversion is the only thing that re-adds one.
C_CARGO = """\
[package]
name = "hegeltest-c"
version = "0.19.1"
edition = "2024"

[dependencies]
ciborium = "0.2.2"
serde = { version = "1.0.103", features = ["derive"] }
"""


class ApplyVersionBumpTest(unittest.TestCase):
    def setUp(self) -> None:
        self._tmp = tempfile.TemporaryDirectory()
        self.root = Path(self._tmp.name)
        (self.root / "Cargo.toml").write_text(ROOT_CARGO)
        (self.root / "hegel-macros").mkdir()
        (self.root / "hegel-macros" / "Cargo.toml").write_text(MACROS_CARGO)
        (self.root / "hegel-c").mkdir()
        (self.root / "hegel-c" / "Cargo.toml").write_text(C_CARGO)

    def tearDown(self) -> None:
        self._tmp.cleanup()

    def test_bumps_every_package_version(self) -> None:
        release.apply_version_bump(self.root, "0.19.2")
        for rel in ["Cargo.toml", "hegel-macros/Cargo.toml", "hegel-c/Cargo.toml"]:
            text = (self.root / rel).read_text()
            self.assertIn('version = "0.19.2"', text, rel)
            self.assertNotIn('version = "0.19.1"', text, rel)

    def test_bumps_internal_path_dependency_pins(self) -> None:
        # The regression: both internal path deps in the root manifest must
        # follow the package version, or `cargo update --workspace` fails.
        release.apply_version_bump(self.root, "0.19.2")
        root_text = (self.root / "Cargo.toml").read_text()
        self.assertIn(
            'hegeltest-macros = { version = "=0.19.2", path = "hegel-macros" }',
            root_text,
        )
        self.assertIn(
            'hegeltest-c = { version = "=0.19.2", path = "hegel-c", '
            "default-features = false }",
            root_text,
        )

    def test_leaves_external_dependencies_untouched(self) -> None:
        release.apply_version_bump(self.root, "0.19.2")
        root_text = (self.root / "Cargo.toml").read_text()
        # External deps carry no `path =`, so they must be left alone — and the
        # `__bench` feature reference is not a version pin either.
        self.assertIn('serde = { version = "1.0.103", features = ["derive"] }', root_text)
        self.assertIn('__bench = ["hegeltest-c/__bench"]', root_text)


class MostSignificantTest(unittest.TestCase):
    def test_picks_the_largest_bump(self) -> None:
        self.assertEqual(release.most_significant(["patch"]), "patch")
        self.assertEqual(release.most_significant(["patch", "minor"]), "minor")
        self.assertEqual(release.most_significant(["minor", "major"]), "major")
        self.assertEqual(release.most_significant(["major", "patch"]), "major")


class PlanReleaseTest(unittest.TestCase):
    def test_root_only_uses_its_content_and_type(self) -> None:
        version, root, c = release.plan_release(
            "0.1.0", ("patch", "This patch does a thing."), None
        )
        self.assertEqual(version, "0.1.1")
        self.assertEqual(root, "This patch does a thing.")
        self.assertIsNone(c)

    def test_c_only_patch_auto_generates_root_entry(self) -> None:
        version, root, c = release.plan_release(
            "0.1.0", None, ("patch", "This patch tweaks the C ABI.")
        )
        self.assertEqual(version, "0.1.1")
        self.assertEqual(
            root, "This release updates the `hegeltest-c` dependency to 0.1.1."
        )
        self.assertEqual(c, "This patch tweaks the C ABI.")

    def test_c_only_minor_bumps_the_shared_version(self) -> None:
        version, root, c = release.plan_release(
            "0.1.0", None, ("minor", "This release breaks the C ABI.")
        )
        self.assertEqual(version, "0.2.0")
        self.assertEqual(
            root, "This release updates the `hegeltest-c` dependency to 0.2.0."
        )
        self.assertEqual(c, "This release breaks the C ABI.")

    def test_both_present_uses_each_content_and_max_bump(self) -> None:
        version, root, c = release.plan_release(
            "0.1.0",
            ("patch", "This patch does a thing."),
            ("minor", "This release breaks the C ABI."),
        )
        self.assertEqual(version, "0.2.0")
        self.assertEqual(root, "This patch does a thing.")
        self.assertEqual(c, "This release breaks the C ABI.")


class BuildReleaseNotesTest(unittest.TestCase):
    def test_root_only_is_passed_through(self) -> None:
        self.assertEqual(release.build_release_notes("root body", None), "root body")

    def test_both_are_combined_under_a_heading(self) -> None:
        notes = release.build_release_notes("root body", "c body")
        self.assertIn("root body", notes)
        self.assertIn("## libhegel C ABI", notes)
        self.assertIn("c body", notes)


if __name__ == "__main__":
    unittest.main()
