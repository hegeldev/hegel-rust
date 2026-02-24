import argparse
import re
import subprocess
from datetime import datetime, timezone
from pathlib import Path

SOURCE_DIRS = ["src/", "hegel-derive/"]
ROOT = Path(__file__).resolve().parent.parent.parent


def git(*args: str, cwd: Path | None = None) -> None:
    subprocess.run(["git", *args], check=True, cwd=cwd)


def parse_release_file(path: Path) -> tuple[str, str]:
    text = path.read_text()
    first_line, _, rest = text.partition("\n")

    match = re.match(r"^RELEASE_TYPE: (major|minor|patch)$", first_line)
    if not match:
        raise ValueError(
            f"Expected RELEASE_TYPE: major|minor|patch, got {first_line!r}"
        )

    content = rest.strip()
    if not content:
        raise ValueError("Changelog cannot be empty.")

    return match.group(1), content


def bump_version(current: str, release_type: str) -> str:
    parts = current.split(".")
    major, minor, patch = int(parts[0]), int(parts[1]), int(parts[2])

    if release_type == "major":
        major += 1
        minor = 0
        patch = 0
    elif release_type == "minor":
        minor += 1
        patch = 0
    else:
        assert release_type == "patch"
        patch += 1

    return f"{major}.{minor}.{patch}"


def set_version(cargo_toml: Path, new_version: str) -> None:
    text = cargo_toml.read_text()
    new_text = re.sub(
        r'^version = "[^"]+"',
        f'version = "{new_version}"',
        text,
        count=1,
        flags=re.MULTILINE,
    )
    cargo_toml.write_text(new_text)


def add_changelog(path: Path, *, version: str, content: str) -> None:
    date = datetime.now(timezone.utc).strftime("%Y-%m-%d")
    entry = f"## {version} - {date}\n\n{content}\n"

    existing = path.read_text()
    assert existing.startswith("# Changelog")
    rest = existing.removeprefix("# Changelog")
    path.write_text(f"# Changelog\n\n{entry}{rest}")


def check(base_ref: str) -> None:
    output = subprocess.check_output(
        ["git", "diff", "--name-only", f"origin/{base_ref}...HEAD"],
        text=True,
        cwd=ROOT,
    )
    changed_files = [line for line in output.splitlines() if line.strip()]

    if not any(f.startswith(d) for f in changed_files for d in SOURCE_DIRS):
        return

    release_file = ROOT / "RELEASE.md"
    if not release_file.exists():
        raise ValueError("Source files changed but no RELEASE.md found.")

    # perform validation of RELEASE.md
    parse_release_file(release_file)


def release() -> None:
    release_file = ROOT / "RELEASE.md"
    assert release_file.exists()

    release_type, content = parse_release_file(release_file)

    m = re.search(
        r'^version = "([^"]+)"', (ROOT / "Cargo.toml").read_text(), re.MULTILINE
    )
    new_version = bump_version(m.group(1), release_type)

    set_version(ROOT / "Cargo.toml", new_version)
    set_version(ROOT / "hegel-derive" / "Cargo.toml", new_version)

    # regenerate lockfile after version bump
    subprocess.run(["cargo", "generate-lockfile"], check=True, cwd=ROOT)

    add_changelog(ROOT / "CHANGELOG.md", version=new_version, content=content)

    git("config", "user.name", "hegel-release[bot]", cwd=ROOT)
    git("config", "user.email", "noreply@github.com", cwd=ROOT)
    git(
        "add",
        "Cargo.toml",
        "Cargo.lock",
        "hegel-derive/Cargo.toml",
        "CHANGELOG.md",
        cwd=ROOT,
    )
    git("rm", "RELEASE.md", cwd=ROOT)
    git(
        "commit",
        "-m",
        f"Bump to version {new_version} and update changelog\n\n[skip ci]",
        cwd=ROOT,
    )
    git("tag", f"v{new_version}", cwd=ROOT)
    git("push", "origin", "main", "--tags", cwd=ROOT)

    subprocess.run(
        [
            "gh",
            "release",
            "create",
            f"v{new_version}",
            "--title",
            f"v{new_version}",
            "--notes",
            content,
        ],
        check=True,
        cwd=ROOT,
    )


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description="Release automation for hegel-rust.")
    subparsers = parser.add_subparsers(dest="command", required=True)

    check_parser = subparsers.add_parser("check")
    check_parser.add_argument("base_ref", help="Git ref to diff against.")
    subparsers.add_parser("release")

    args = parser.parse_args()
    if args.command == "check":
        check(args.base_ref)
    elif args.command == "release":
        release()
