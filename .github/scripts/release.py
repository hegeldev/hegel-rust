import argparse
import os
import re
import subprocess
from datetime import datetime, timezone
from pathlib import Path

# Changes under these dirs require a hegel-rust RELEASE.md (the root one,
# feeding CHANGELOG.md). Changes under C_SOURCE_DIRS require hegel-c/RELEASE.md
# (feeding hegel-c/CHANGELOG.md) instead.
RUST_SOURCE_DIRS = ["src/", "hegel-macros/"]
C_SOURCE_DIRS = ["hegel-c/src/"]
ROOT = Path(__file__).resolve().parent.parent.parent

ROOT_RELEASE = "RELEASE.md"
C_RELEASE = "hegel-c/RELEASE.md"
ROOT_CHANGELOG = "CHANGELOG.md"
C_CHANGELOG = "hegel-c/CHANGELOG.md"

# hegel-rust (the `hegeltest` crate) and hegel-c (`hegeltest-c`) carry
# independent version numbers, because a change can be breaking for one without
# being breaking for the other (a breaking C ABI change is a minor bump for
# hegel-c but only a patch for hegel-rust). The root and hegel-macros manifests
# share the hegel-rust version; hegel-c has its own.

# Files the release commit reads, rewrites, and stages. These are validated by
# `check` on every PR so removing one (as the conformance-test removal did with
# tests/conformance/rust) fails fast instead of breaking the actual release —
# which only ever runs the `release` subcommand on a push to main.
RELEASE_PATHS = [
    "Cargo.toml",
    "Cargo.lock",
    "hegel-macros/Cargo.toml",
    "hegel-c/Cargo.toml",
    "CHANGELOG.md",
    "hegel-c/CHANGELOG.md",
]


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


def current_version(cargo_toml: Path) -> str:
    """The `version = "..."` of the package manifest at `cargo_toml`."""
    m = re.search(r'^version = "([^"]+)"', cargo_toml.read_text(), re.MULTILINE)
    return m.group(1)


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


def bump_internal_path_deps(cargo_toml: Path, versions: dict[str, str]) -> None:
    """Bump each internal (path) dependency pinned with `=` to its new version.

    Internal crates are pinned exactly (`version = "=X.Y.Z"`) and declared with
    a `path = ...`. `versions` maps a crate name to the version its pin should
    move to. Requiring both an exact pin and a `path =` (rather than trusting a
    hardcoded crate list) means a change in dependency direction can't silently
    leave a stale pin behind — which is exactly how the inversion refactor broke
    the release: the root crate gained a `hegeltest-c` path dependency that the
    old hardcoded list never touched. Now that hegel-rust and hegel-c version
    independently, the pin is keyed by crate name so each follows its own crate.
    """
    lines = cargo_toml.read_text().splitlines(keepends=True)
    out = []
    for line in lines:
        match = re.match(r"\s*([A-Za-z0-9_-]+)\s*=", line)
        if (
            match
            and match.group(1) in versions
            and "path =" in line
            and re.search(r'version = "=[^"]+"', line)
        ):
            line = re.sub(
                r'version = "=[^"]+"',
                f'version = "={versions[match.group(1)]}"',
                line,
            )
        out.append(line)
    cargo_toml.write_text("".join(out))


def apply_version_bump(root: Path, rust_version: str, c_version: str) -> None:
    """Rewrite the workspace manifests to their new versions.

    The root and hegel-macros manifests move to `rust_version`; hegel-c moves to
    `c_version`. Internal path-dependency pins follow the crate they point at.
    Pure file rewriting, factored out of `release()` so it can be tested without
    the publish/git/gh machinery around it.
    """
    versions = {"hegeltest-macros": rust_version, "hegeltest-c": c_version}

    set_version(root / "Cargo.toml", rust_version)
    set_version(root / "hegel-macros" / "Cargo.toml", rust_version)
    set_version(root / "hegel-c" / "Cargo.toml", c_version)

    for manifest in [
        root / "Cargo.toml",
        root / "hegel-macros" / "Cargo.toml",
        root / "hegel-c" / "Cargo.toml",
    ]:
        bump_internal_path_deps(manifest, versions)


def add_changelog(path: Path, *, version: str, content: str) -> None:
    date = datetime.now(timezone.utc).strftime("%Y-%m-%d")
    entry = f"## {version} - {date}\n\n{content}"

    existing = path.read_text()
    assert existing.startswith("# Changelog")
    rest = existing.removeprefix("# Changelog")
    path.write_text(f"# Changelog\n\n{entry}{rest}")


def plan_release(
    rust_current: str,
    c_current: str,
    root_release: tuple[str, str] | None,
    c_release: tuple[str, str] | None,
) -> tuple[str, str, str, str | None]:
    """Decide the new versions and changelog bodies for a release.

    `root_release`/`c_release` are `(release_type, content)` pairs parsed from
    RELEASE.md / hegel-c/RELEASE.md, or None when that file is absent. At least
    one must be present (the release only runs when one exists).

    The two crates version independently. hegel-c bumps by its own release file.
    hegel-rust bumps by its release file when present; otherwise — when only
    hegel-c changed — it takes a patch bump because the root crate pins
    `hegeltest-c` exactly and must be republished against the new engine, and
    its changelog gets an auto-generated dependency-bump entry. That entry is
    only correct when there are no functional hegel-rust changes (see the
    changelog skill).

    Returns `(rust_version, c_version, root_content, c_content)`; `c_content`
    is None (and `c_version` unchanged) when there is no hegel-c/RELEASE.md.
    """
    assert root_release is not None or c_release is not None

    if c_release is not None:
        c_version = bump_version(c_current, c_release[0])
        c_content = c_release[1]
    else:
        c_version = c_current
        c_content = None

    if root_release is not None:
        rust_version = bump_version(rust_current, root_release[0])
        root_content = root_release[1]
    else:
        rust_version = bump_version(rust_current, "patch")
        root_content = (
            f"This release updates the `hegeltest-c` dependency to {c_version}."
        )

    return rust_version, c_version, root_content, c_content


def build_release_notes(root_content: str, c_content: str | None) -> str:
    """Combine the changelog bodies into the GitHub release notes."""
    if c_content is None:
        return root_content
    return f"{root_content}\n\n## libhegel C ABI\n\n{c_content}"


def check(base_ref: str) -> None:
    missing = [rel for rel in RELEASE_PATHS if not (ROOT / rel).exists()]
    if missing:
        raise ValueError(
            "release.py would fail: these paths it stages no longer exist: "
            + ", ".join(missing)
        )

    output = subprocess.check_output(
        ["git", "diff", "--name-only", f"origin/{base_ref}...HEAD"],
        text=True,
        cwd=ROOT,
    )
    changed_files = [line for line in output.splitlines() if line.strip()]

    rust_changed = any(
        f.startswith(d) for f in changed_files for d in RUST_SOURCE_DIRS
    )
    c_changed = any(f.startswith(d) for f in changed_files for d in C_SOURCE_DIRS)

    # A hegel-c release auto-generates the root dependency-bump entry, so when
    # hegel-c is releasing (its RELEASE.md is present) a root RELEASE.md is
    # optional — the author writes one only when there are functional hegel-rust
    # changes to document. A hegel-rust change with no hegel-c release has no
    # such fallback, so the root RELEASE.md is required.
    c_release_present = (ROOT / C_RELEASE).exists()

    require_release_file(
        base_ref,
        ROOT_RELEASE,
        changed_files,
        [
            "Every pull request to hegel-rust requires a RELEASE.md file.",
            "You can find an example and instructions in RELEASE-sample.md.",
        ],
        required=rust_changed and not c_release_present,
    )

    require_release_file(
        base_ref,
        C_RELEASE,
        changed_files,
        [
            "Every pull request changing the libhegel C ABI requires a "
            "hegel-c/RELEASE.md file.",
            "You can find an example and instructions in RELEASE-sample.md.",
        ],
        required=c_changed,
    )


def require_release_file(
    base_ref: str,
    rel_path: str,
    changed_files: list[str],
    missing_lines: list[str],
    *,
    required: bool,
) -> None:
    """Validate a release file, erroring if it is `required` but absent.

    A present file is always validated (and guarded against being a leftover
    from a failed release); an absent file errors only when `required`.
    """
    release_file = ROOT / rel_path
    if not release_file.exists():
        if not required:
            return
        width = max(len(l) for l in missing_lines) + 6
        border = " ".join("*" * ((width + 1) // 2))
        empty = "*" + " " * (width - 2) + "*"
        inner = "\n".join("*" + l.center(width - 2) + "*" for l in missing_lines)
        pad = "\t"
        box = f"\n{pad}{border}\n{pad}{empty}\n{pad}{empty}\n"
        box += "\n".join(f"{pad}" + l for l in inner.split("\n"))
        box += f"\n{pad}{empty}\n{pad}{empty}\n{pad}{border}\n"
        raise ValueError(box)

    process = subprocess.run(
        ["git", "cat-file", "-e", f"origin/{base_ref}:{rel_path}"],
        capture_output=True,
        cwd=ROOT,
    )
    if process.returncode == 0 and rel_path not in changed_files:
        raise ValueError(
            f"{rel_path} already exists on {base_ref}. It's possible the CI job "
            "responsible for cutting a new release is in progress, or has failed."
        )

    parse_release_file(release_file)


def release() -> None:
    root_path = ROOT / ROOT_RELEASE
    c_path = ROOT / C_RELEASE
    root_release = parse_release_file(root_path) if root_path.exists() else None
    c_release = parse_release_file(c_path) if c_path.exists() else None
    assert root_release is not None or c_release is not None

    rust_current = current_version(ROOT / "Cargo.toml")
    c_current = current_version(ROOT / "hegel-c" / "Cargo.toml")
    rust_version, c_version, root_content, c_content = plan_release(
        rust_current, c_current, root_release, c_release
    )

    apply_version_bump(ROOT, rust_version, c_version)

    # regenerate the lockfile after version bump
    subprocess.run(["cargo", "update", "--workspace"], check=True, cwd=ROOT)

    add_changelog(ROOT / ROOT_CHANGELOG, version=rust_version, content=root_content)
    if c_content is not None:
        add_changelog(ROOT / C_CHANGELOG, version=c_version, content=c_content)

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
    git("add", *RELEASE_PATHS, cwd=ROOT)
    if root_release is not None:
        git("rm", ROOT_RELEASE, cwd=ROOT)
    if c_release is not None:
        git("rm", C_RELEASE, cwd=ROOT)

    bumped = [f"hegeltest to {rust_version}"]
    if c_release is not None:
        bumped.append(f"hegeltest-c to {c_version}")
    git(
        "commit",
        "-m",
        f"Bump {' and '.join(bumped)} and update changelog\n\n[skip ci]",
        cwd=ROOT,
    )

    # The GitHub release carries the libhegel shared libraries, so it is the
    # hegel-c release: it is tagged with the hegel-c version and only cut when
    # hegel-c changed. A hegel-rust-only release just publishes to crates.io and
    # updates the changelog — no tag, no GitHub release.
    if c_release is not None:
        git("tag", f"v{c_version}", cwd=ROOT)
        git("push", "origin", f"v{c_version}", cwd=ROOT)
        subprocess.run(
            [
                "gh",
                "release",
                "create",
                f"v{c_version}",
                "--draft",
                "--title",
                f"v{c_version}",
                "--notes",
                build_release_notes(root_content, c_content),
            ],
            check=True,
            cwd=ROOT,
        )


def release_pr_details(version: str, tags: list[str]) -> tuple[str, str]:
    """Title and body for the fallback PR opened when the release push races a
    concurrent merge to main.

    `version` is the hegeltest version of the release commit, which names the
    PR. `tags` is whatever release tags point at that commit: the hegel-c
    version tag when hegel-c released, and nothing for a hegel-rust-only
    release, which cuts no tag — so the body only claims a tag was pushed when
    one actually was, and names it (the tag carries the hegel-c version, not
    `version`).
    """
    title = f"Release v{version}"
    if tags:
        pushed = f"after tagging {' and '.join(tags)} "
        succeeded = "The tag and crates.io publish succeeded."
    else:
        pushed = ""
        succeeded = "The crates.io publish succeeded."
    body = (
        f"The push to main {pushed}failed because main had diverged. "
        f"{succeeded}\n\n"
        f"This PR merges the release commit (version bump, changelog, "
        f"RELEASE.md removal) into main."
    )
    return title, body


def push_or_pr() -> None:
    version = current_version(ROOT / "Cargo.toml")

    result = subprocess.run(
        ["git", "push", "origin", "main"], cwd=ROOT
    )
    if result.returncode == 0:
        return

    print(f"Push to main failed, creating PR for release v{version}")

    branch = f"release/v{version}"
    git("checkout", "-b", branch, cwd=ROOT)
    git("push", "origin", branch, cwd=ROOT)

    # Ensure the "skip release" label exists so check-release doesn't run on this PR
    subprocess.run(
        [
            "gh", "label", "create", "skip release",
            "--force",
            "--description", "Skip the release check on this PR",
        ],
        cwd=ROOT,
    )

    tags = subprocess.check_output(
        ["git", "tag", "--points-at", "HEAD"], cwd=ROOT, text=True
    ).split()
    title, body = release_pr_details(version, tags)
    subprocess.run(
        [
            "gh", "pr", "create",
            "--base", "main",
            "--head", branch,
            "--title", title,
            "--body", body,
            "--label", "skip release",
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

    subparsers.add_parser("push-or-pr")

    args = parser.parse_args()
    if args.command == "check":
        check(args.base_ref)
    elif args.command == "release":
        release()
    elif args.command == "push-or-pr":
        push_or_pr()
