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
hegeltest-c = { version = "=0.5.0", path = "hegel-c", default-features = false }
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
version = "0.5.0"
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

    def test_bumps_each_package_to_its_own_version(self) -> None:
        # hegel-rust (root + macros) and hegel-c version independently.
        release.apply_version_bump(self.root, "0.19.2", "0.6.0")
        for rel in ["Cargo.toml", "hegel-macros/Cargo.toml"]:
            text = (self.root / rel).read_text()
            self.assertIn('version = "0.19.2"', text, rel)
            self.assertNotIn('version = "0.19.1"', text, rel)
        c_text = (self.root / "hegel-c" / "Cargo.toml").read_text()
        self.assertIn('version = "0.6.0"', c_text)
        self.assertNotIn('version = "0.5.0"', c_text)

    def test_bumps_internal_path_dependency_pins_per_crate(self) -> None:
        # Each internal path dep in the root manifest must follow its own
        # crate's version, or `cargo update --workspace` fails: the macros pin
        # tracks the hegel-rust version, the hegeltest-c pin the hegel-c version.
        release.apply_version_bump(self.root, "0.19.2", "0.6.0")
        root_text = (self.root / "Cargo.toml").read_text()
        self.assertIn(
            'hegeltest-macros = { version = "=0.19.2", path = "hegel-macros" }',
            root_text,
        )
        self.assertIn(
            'hegeltest-c = { version = "=0.6.0", path = "hegel-c", '
            "default-features = false }",
            root_text,
        )

    def test_leaves_external_dependencies_untouched(self) -> None:
        release.apply_version_bump(self.root, "0.19.2", "0.6.0")
        root_text = (self.root / "Cargo.toml").read_text()
        # External deps carry no `path =`, so they must be left alone — and the
        # `__bench` feature reference is not a version pin either.
        self.assertIn('serde = { version = "1.0.103", features = ["derive"] }', root_text)
        self.assertIn('__bench = ["hegeltest-c/__bench"]', root_text)


class PlanReleaseTest(unittest.TestCase):
    def test_root_only_bumps_rust_and_leaves_c(self) -> None:
        rust, c, root_body, c_body = release.plan_release(
            "0.19.1", "0.5.0", ("patch", "This patch does a thing."), None
        )
        self.assertEqual(rust, "0.19.2")
        self.assertEqual(c, "0.5.0")
        self.assertEqual(root_body, "This patch does a thing.")
        self.assertIsNone(c_body)

    def test_c_only_patch_bumps_both_and_auto_generates_root_entry(self) -> None:
        rust, c, root_body, c_body = release.plan_release(
            "0.19.1", "0.5.0", None, ("patch", "This patch tweaks the C ABI.")
        )
        self.assertEqual(rust, "0.19.2")
        self.assertEqual(c, "0.5.1")
        self.assertEqual(
            root_body, "This release updates the `hegeltest-c` dependency to 0.5.1."
        )
        self.assertEqual(c_body, "This patch tweaks the C ABI.")

    def test_c_minor_is_only_a_rust_patch(self) -> None:
        # The whole point of independent versions: a breaking C ABI change is a
        # minor bump for hegel-c but only a patch for hegel-rust.
        rust, c, root_body, c_body = release.plan_release(
            "0.19.1", "0.5.0", None, ("minor", "This release breaks the C ABI.")
        )
        self.assertEqual(rust, "0.19.2")
        self.assertEqual(c, "0.6.0")
        self.assertEqual(
            root_body, "This release updates the `hegeltest-c` dependency to 0.6.0."
        )
        self.assertEqual(c_body, "This release breaks the C ABI.")

    def test_both_present_bump_independently(self) -> None:
        rust, c, root_body, c_body = release.plan_release(
            "0.19.1",
            "0.5.0",
            ("patch", "This patch does a thing."),
            ("minor", "This release breaks the C ABI."),
        )
        self.assertEqual(rust, "0.19.2")
        self.assertEqual(c, "0.6.0")
        self.assertEqual(root_body, "This patch does a thing.")
        self.assertEqual(c_body, "This release breaks the C ABI.")


class CurrentVersionTest(unittest.TestCase):
    def test_reads_the_package_version(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            manifest = Path(tmp) / "Cargo.toml"
            manifest.write_text(ROOT_CARGO)
            self.assertEqual(release.current_version(manifest), "0.19.1")


class ReleasePrDetailsTest(unittest.TestCase):
    def test_rust_only_release_mentions_no_tag(self) -> None:
        title, body = release.release_pr_details("0.23.3", [])
        self.assertEqual(title, "Release v0.23.3")
        self.assertNotIn("tag", body)
        self.assertIn("The crates.io publish succeeded.", body)

    def test_tagged_release_names_the_pushed_tag(self) -> None:
        title, body = release.release_pr_details("0.23.3", ["v0.24.0"])
        self.assertEqual(title, "Release v0.23.3")
        self.assertIn("after tagging v0.24.0", body)
        self.assertIn("The tag and crates.io publish succeeded.", body)


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
