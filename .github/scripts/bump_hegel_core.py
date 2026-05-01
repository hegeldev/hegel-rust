import json
import os
import re
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent.parent
CORE_REPO = "hegeldev/hegel-core"

# Optional Rust visibility modifier: `pub`, `pub(crate)`, `pub(super)`, `pub(in path)`.
_VISIBILITY = r"(?:pub(?:\([^)]+\))?\s+)?"

_VERSION_VALUE_RE = re.compile(
    rf'^{_VISIBILITY}const HEGEL_SERVER_VERSION: &str = "([^"]+)";',
    re.MULTILINE,
)
_VERSION_SUB_RE = re.compile(
    rf'^({_VISIBILITY}const HEGEL_SERVER_VERSION: &str = ")[^"]+(";)',
    re.MULTILINE,
)
_PROTOCOL_SUB_RE = re.compile(
    rf'^({_VISIBILITY}const SUPPORTED_PROTOCOL_VERSIONS: \(&str, &str\) = \("[^"]+", ")[^"]+("\);)',
    re.MULTILINE,
)
_FLAKE_TAG_SUB_RE = re.compile(r"refs/tags/v[0-9.]+")


def git(*args: str) -> None:
    subprocess.run(["git", *args], check=True, cwd=ROOT)


def parse_current_version(session_rs_text: str) -> str:
    m = _VERSION_VALUE_RE.search(session_rs_text)
    if m is None:
        raise ValueError(
            "Could not find `const HEGEL_SERVER_VERSION: &str = \"...\";` in session.rs. "
            "The constant may have been renamed or moved — update the regex accordingly."
        )
    return m.group(1)


def update_session(session_rs_text: str, version: str, protocol_version: str) -> str:
    new_text, n = _VERSION_SUB_RE.subn(
        rf"\g<1>{version}\g<2>", session_rs_text, count=1
    )
    if n != 1:
        raise ValueError("Could not find `const HEGEL_SERVER_VERSION` to update.")
    new_text, n = _PROTOCOL_SUB_RE.subn(
        rf"\g<1>{protocol_version}\g<2>", new_text, count=1
    )
    if n != 1:
        raise ValueError(
            "Could not find `const SUPPORTED_PROTOCOL_VERSIONS` to update."
        )
    return new_text


def update_flake(flake_nix_text: str, version: str) -> str:
    new_text, n = _FLAKE_TAG_SUB_RE.subn(
        f"refs/tags/v{version}", flake_nix_text, count=1
    )
    if n != 1:
        raise ValueError("Could not find `refs/tags/v...` in flake.nix.")
    return new_text


def format_release_md(version: str, releases: list[dict[str, str]]) -> str:
    release_url = f"https://github.com/{CORE_REPO}/releases/tag/v{version}"

    changelog_sections = []
    for r in releases:
        url = f"https://github.com/{CORE_REPO}/releases/tag/v{r['version']}"
        quoted = "\n".join(f"> {line}" if line else ">" for line in r["body"].splitlines())
        changelog_sections.append(f"{quoted}\n>\n> — [v{r['version']}]({url})")

    changes_text = "\n\n".join(changelog_sections)
    noun = "change" if len(releases) == 1 else "changes"

    return (
        f"RELEASE_TYPE: patch\n\n"
        f"Bump our pinned hegel-core to [{version}]({release_url}), "
        f"incorporating the following {noun}:\n\n"
        f"{changes_text}\n"
    )


def get_current_version() -> str:
    return parse_current_version((ROOT / "src" / "server" / "session.rs").read_text())


def get_releases_in_range(from_version: str, to_version: str) -> list[dict[str, str]]:
    """Fetch hegel-core releases between from_version (exclusive) and to_version (inclusive)."""
    result = subprocess.run(
        ["gh", "api", f"repos/{CORE_REPO}/releases", "--paginate", "--jq", ".[]"],
        capture_output=True,
        text=True,
        check=True,
        cwd=ROOT,
    )
    # --jq ".[]" with --paginate outputs one JSON object per line
    releases = [json.loads(line) for line in result.stdout.strip().splitlines() if line.strip()]

    from_parts = [int(x) for x in from_version.split(".")]
    to_parts = [int(x) for x in to_version.split(".")]

    in_range = []
    for release in releases:
        tag = release["tag_name"].lstrip("v")
        parts = [int(x) for x in tag.split(".")]
        if parts > from_parts and parts <= to_parts:
            in_range.append({"version": tag, "body": release["body"].strip()})

    # Sort oldest first
    in_range.sort(key=lambda r: [int(x) for x in r["version"].split(".")])
    return in_range


def bump(version: str, protocol_version: str) -> None:
    current_version = get_current_version()

    session = ROOT / "src" / "server" / "session.rs"
    session.write_text(update_session(session.read_text(), version, protocol_version))

    flake = ROOT / "nix" / "flake.nix"
    flake.write_text(update_flake(flake.read_text(), version))

    subprocess.run(
        ["nix", "--extra-experimental-features", "nix-command flakes", "flake", "lock", "./nix"],
        check=True,
        cwd=ROOT,
    )

    releases = get_releases_in_range(current_version, version)
    release_md = ROOT / "RELEASE.md"
    release_md.write_text(format_release_md(version, releases))

    app_id = os.environ["HEGEL_RELEASE_APP_ID"]
    git("config", "user.name", "hegel-release[bot]")
    git("config", "user.email", f"{app_id}+hegel-release[bot]@users.noreply.github.com")

    git("checkout", "-b", "ci/bump-hegel-core")
    git("add", "src/server/session.rs", "nix/flake.nix", "nix/flake.lock", "RELEASE.md")
    git("commit", "-m", f"Bump hegel-core to {version}")
    git("push", "--force", "origin", "ci/bump-hegel-core")

    # Only create a PR if one doesn't already exist for this branch.
    # If one exists, the force-push above already updated it.
    result = subprocess.run(
        ["gh", "pr", "list", "--head", "ci/bump-hegel-core", "--state", "open", "--json", "number"],
        capture_output=True,
        text=True,
        cwd=ROOT,
    )
    has_open_pr = result.returncode == 0 and result.stdout.strip() not in ("", "[]")

    title = f"Bump pinned `hegel-core` to `{version}`"
    bump_url = "https://github.com/hegeldev/hegel-rust/blob/main/.github/workflows/bump-hegel-core.yml"
    core_url = "https://github.com/hegeldev/hegel-core/blob/main/.github/workflows/ci.yml"
    body = (
        f"This PR bumps our pinned `hegel-core` version to `v{version}`.\n"
        "\n"
        "---\n"
        "\n"
        f"*This PR was automatically generated by [bump-hegel-core.yml]({bump_url})"
        f" after a trigger by [this hegel-core workflow]({core_url}).*"
    )
    if has_open_pr:
        subprocess.run(
            ["gh", "pr", "edit", "ci/bump-hegel-core", "--title", title, "--body", body],
            check=True,
            cwd=ROOT,
        )
    else:
        subprocess.run(
            ["gh", "pr", "create", "--title", title, "--body", body],
            check=True,
            cwd=ROOT,
        )


if __name__ == "__main__":
    bump(os.environ["NEW_VERSION"], os.environ["NEW_PROTOCOL_VERSION"])
