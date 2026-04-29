import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / ".github" / "scripts"))

import bump_hegel_core  # noqa: E402  # pyright: ignore[reportMissingImports]


def test_bump_runs_against_real_files() -> None:
    # Regression: catch refactors that change session.rs / flake.nix in a way
    # the bump script's regexes no longer match. Without this the breakage is
    # invisible until the next hegel-core release fires the workflow.
    bump_hegel_core.update_session(
        (bump_hegel_core.ROOT / "src" / "server" / "session.rs").read_text(),
        "9.9.9",
        "0.99",
    )
    bump_hegel_core.update_flake(
        (bump_hegel_core.ROOT / "nix" / "flake.nix").read_text(),
        "9.9.9",
    )
