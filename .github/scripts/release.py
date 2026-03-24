import argparse
import os
import re
import subprocess
from datetime import datetime, timezone
from pathlib import Path

SOURCE_DIRS = ["src/", "hegel-macros/"]
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


def set_macros_dep_version(cargo_toml: Path, new_version: str) -> None:
    text = cargo_toml.read_text()
    new_text = re.sub(
        r'hegeltest-macros = \{ version = "=[^"]+"',
        f'hegeltest-macros = {{ version = "={new_version}"',
        text,
        count=1,
    )
    cargo_toml.write_text(new_text)


def set_lib_doc_version(lib_rs: Path, new_version: str) -> None:
    text = lib_rs.read_text()
    new_text = re.sub(
        r'//! hegeltest = "[^"]+"',
        f'//! hegeltest = "{new_version}"',
        text,
        count=1,
    )
    lib_rs.write_text(new_text)


def add_changelog(path: Path, *, version: str, content: str) -> None:
    date = datetime.now(timezone.utc).strftime("%Y-%m-%d")
    entry = f"## {version} - {date}\n\n{content}"

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

    process = subprocess.run(
        ["git", "cat-file", "-e", f"origin/{base_ref}:RELEASE.md"],
        capture_output=True,
        cwd=ROOT,
    )
    if process.returncode == 0:
        raise ValueError(
            f"RELEASE.md already exists on {base_ref}. It's possible the CI job "
            "responsible for cutting a new release is in progress, or has failed."
        )

    if not release_file.exists():
        lines = [
            "Every pull request to hegel-rust requires a RELEASE.md file.",
            "You can find an example and instructions in RELEASE-sample.md.",
        ]
        width = max(len(l) for l in lines) + 6
        border = " ".join("*" * ((width + 1) // 2))
        empty = "*" + " " * (width - 2) + "*"
        inner = "\n".join("*" + l.center(width - 2) + "*" for l in lines)
        pad = "\t"
        box = f"\n{pad}{border}\n{pad}{empty}\n{pad}{empty}\n"
        box += "\n".join(f"{pad}" + l for l in inner.split("\n"))
        box += f"\n{pad}{empty}\n{pad}{empty}\n{pad}{border}\n"
        raise ValueError(box)

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
    set_version(ROOT / "hegel-macros" / "Cargo.toml", new_version)
    set_macros_dep_version(ROOT / "Cargo.toml", new_version)

    # regenerate lockfiles after version bump
    subprocess.run(["cargo", "generate-lockfile"], check=True, cwd=ROOT)
    subprocess.run(
        ["cargo", "generate-lockfile"],
        check=True,
        cwd=(ROOT / "tests" / "conformance" / "rust"),
    )

    set_lib_doc_version(ROOT / "src" / "lib.rs", new_version)
    add_changelog(ROOT / "CHANGELOG.md", version=new_version, content=content)

    app_slug = os.environ["HEGEL_RELEASE_APP_SLUG"]
    bot_user_id = subprocess.check_output(
        ["gh", "api", f"/users/{app_slug}[bot]", "--jq", ".id"], text=True
    ).strip()
    git("config", "user.name", f"{app_slug}[bot]", cwd=ROOT)
    git(
        "config",
        "user.email",
        f"{bot_user_id}+{app_slug}[bot]@users.noreply.github.com",
        cwd=ROOT,
    )
    git(
        "add",
        "Cargo.toml",
        "Cargo.lock",
        "hegel-macros/Cargo.toml",
        "tests/conformance/rust/Cargo.lock",
        "src/lib.rs",
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
