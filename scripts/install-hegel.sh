#!/usr/bin/env bash
set -euo pipefail

# install-hegel.sh — Install hegel-core into a shared per-user cache directory.
#
# Required environment:
#   HEGEL_VERSION — The git tag to install (e.g. "v0.4.0")
#
# Prints the absolute path to the installed hegel binary on stdout.
# All other output goes to stderr.

if [ -z "${HEGEL_VERSION:-}" ]; then
    echo "Error: HEGEL_VERSION environment variable must be set" >&2
    echo "Example: HEGEL_VERSION=v0.4.0 bash install-hegel.sh" >&2
    exit 1
fi

if ! command -v uv >/dev/null 2>&1; then
    echo "Error: 'uv' is required but not found on PATH" >&2
    echo "Install it from: https://docs.astral.sh/uv/" >&2
    exit 1
fi

# Compute platform-specific cache directory
case "$(uname -s)" in
    Darwin)
        cache_base="$HOME/Library/Caches/hegel"
        ;;
    *)
        cache_base="${XDG_CACHE_HOME:-$HOME/.cache}/hegel"
        ;;
esac

version_dir="$cache_base/versions/$HEGEL_VERSION"
hegel_bin="$version_dir/venv/bin/hegel"
complete_marker="$version_dir/.complete"

# Fast path: already installed
if [ -f "$complete_marker" ] && [ -x "$hegel_bin" ]; then
    echo "$hegel_bin"
    exit 0
fi

# Clean up stale temp dirs from previous failed installs
mkdir -p "$cache_base/versions"
find "$cache_base/versions" -maxdepth 1 -name '.install-*' -type d -exec rm -rf {} + 2>/dev/null || true

# Concurrency: We can't install to a temp dir and atomically rename it into
# place, because uv bakes absolute paths into entry-point shebangs — moving
# the venv after install breaks the hegel binary. So we install directly into
# the final version_dir and use a lock + completion marker to coordinate
# concurrent processes. mkdir is used as the lock primitive because flock is
# not available on macOS.
lockdir="$cache_base/versions/.lock-$HEGEL_VERSION"

acquire_lock() {
    local attempts=0
    while ! mkdir "$lockdir" 2>/dev/null; do
        attempts=$((attempts + 1))
        if [ "$attempts" -ge 300 ]; then
            echo "Error: timed out waiting for lock on $lockdir" >&2
            exit 1
        fi
        sleep 1
    done
}

release_lock() {
    rmdir "$lockdir" 2>/dev/null || true
}

acquire_lock
trap release_lock EXIT

# Re-check after acquiring lock (another process may have completed installation)
if [ -f "$complete_marker" ] && [ -x "$hegel_bin" ]; then
    echo "$hegel_bin"
    exit 0
fi

# Clean up incomplete installs (version dir exists but not marked complete)
if [ -d "$version_dir" ] && [ ! -f "$complete_marker" ]; then
    rm -rf "$version_dir"
fi

# Create version directory
mkdir -p "$version_dir"

cleanup_on_failure() {
    # Remove the version dir if installation didn't complete
    if [ -d "$version_dir" ] && [ ! -f "$complete_marker" ]; then
        rm -rf "$version_dir"
    fi
    release_lock
}
trap cleanup_on_failure EXIT

echo "Installing hegel $HEGEL_VERSION..." >&2

uv venv "$version_dir/venv" >&2
uv pip install \
    --python "$version_dir/venv/bin/python" \
    "hegel @ git+https://github.com/hegeldev/hegel-core@$HEGEL_VERSION" >&2

if [ ! -x "$hegel_bin" ]; then
    echo "Error: hegel binary not found after installation" >&2
    exit 1
fi

# Mark installation as complete
touch "$complete_marker"

echo "$hegel_bin"
