#!/usr/bin/env bash
# Bootstrap a fresh Ubuntu box for running scripts/port-loop.py against
# hegel-rust. Reconstructed from the bash history of the currently-
# working box. Safe to re-run (idempotent where possible).
#
# Usage:
#   curl -fsSL <raw-url>/scripts/setup-machine.sh | bash
# or:
#   ./scripts/setup-machine.sh

set -euo pipefail

# -----------------------------------------------------------------------------
# 0. Config — edit these before running, or set in the environment.
# -----------------------------------------------------------------------------
GIT_USER_NAME="${GIT_USER_NAME:-David R. MacIver}"
GIT_USER_EMAIL="${GIT_USER_EMAIL:-david.maciver@antithesis.com}"
REPO_URL_SSH="${REPO_URL_SSH:-git@github.com:hegeldev/hegel-rust.git}"
REPO_URL_HTTPS="${REPO_URL_HTTPS:-https://github.com/hegeldev/hegel-rust.git}"
REPO_DIR="${REPO_DIR:-$HOME/hegel-rust}"

log() { printf '\n[setup] %s\n' "$*"; }

# -----------------------------------------------------------------------------
# 1. APT packages.
# -----------------------------------------------------------------------------
log "installing apt packages"
sudo apt update
sudo apt install -y \
    build-essential \
    curl \
    git \
    gh \
    jq \
    tmux \
    vim \
    pkg-config \
    libssl-dev \
    ca-certificates

# -----------------------------------------------------------------------------
# 2. Rust toolchain.
# -----------------------------------------------------------------------------
if ! command -v rustup >/dev/null 2>&1; then
    log "installing rustup"
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
        | sh -s -- -y --default-toolchain stable --profile default
fi
# shellcheck disable=SC1091
source "$HOME/.cargo/env"
rustup update stable
rustup component add llvm-tools-preview

# -----------------------------------------------------------------------------
# 3. `just` (task runner used by `just check`, `just coverage`, etc).
# -----------------------------------------------------------------------------
if ! command -v just >/dev/null 2>&1; then
    log "installing just"
    cargo install just
fi

# Useful for `just check-coverage`.
if ! command -v cargo-llvm-cov >/dev/null 2>&1; then
    log "installing cargo-llvm-cov"
    cargo install cargo-llvm-cov
fi

# sccache — shared compiler cache used by port-loop.py workers so repeated
# dependency compilations become cache hits across parallel workers.
if ! command -v sccache >/dev/null 2>&1; then
    log "installing sccache"
    cargo install sccache
fi

# -----------------------------------------------------------------------------
# 4. `uv` — required by scripts/port-loop.py (shebang uses `uv run --script`).
# -----------------------------------------------------------------------------
if ! command -v uv >/dev/null 2>&1; then
    log "installing uv"
    curl -LsSf https://astral.sh/uv/install.sh | sh
fi
# uv installs to ~/.local/bin; make sure that's on PATH for this shell.
export PATH="$HOME/.local/bin:$PATH"
if ! grep -q '\.local/bin' "$HOME/.bashrc"; then
    echo 'export PATH="$HOME/.local/bin:$PATH"' >> "$HOME/.bashrc"
fi

# -----------------------------------------------------------------------------
# 5. Claude Code CLI.
# -----------------------------------------------------------------------------
if ! command -v claude >/dev/null 2>&1; then
    log "installing Claude Code"
    curl -fsSL https://claude.ai/install.sh | bash
fi

# -----------------------------------------------------------------------------
# 6. Git identity.
# -----------------------------------------------------------------------------
log "configuring git identity"
git config --global user.name "$GIT_USER_NAME"
git config --global user.email "$GIT_USER_EMAIL"
git config --global init.defaultBranch main
git config --global pull.rebase true

# -----------------------------------------------------------------------------
# 7. GitHub auth.
# Uses `gh auth login` interactively; also adds an SSH key to the GitHub
# account so the `git@github.com:` remote works. Skipped if already
# authenticated.
# -----------------------------------------------------------------------------
if ! gh auth status >/dev/null 2>&1; then
    log "authenticating with GitHub (interactive)"
    echo "Follow the prompts to log in. Pick SSH when asked about the"
    echo "preferred protocol — the repo uses an SSH remote."
    gh auth login -p ssh -w
fi

# -----------------------------------------------------------------------------
# 8. Clone hegel-rust and its `resources/` reference repos.
# -----------------------------------------------------------------------------
if [[ ! -d "$REPO_DIR" ]]; then
    log "cloning hegel-rust into $REPO_DIR"
    if git ls-remote "$REPO_URL_SSH" >/dev/null 2>&1; then
        git clone "$REPO_URL_SSH" "$REPO_DIR"
    else
        log "SSH clone failed; falling back to HTTPS"
        git clone "$REPO_URL_HTTPS" "$REPO_DIR"
    fi
fi
cd "$REPO_DIR"

mkdir -p resources
if [[ ! -d resources/hypothesis ]]; then
    log "cloning upstream Hypothesis"
    git clone --depth 1 https://github.com/HypothesisWorks/hypothesis.git \
        resources/hypothesis
fi
if [[ ! -d resources/pbtkit ]]; then
    log "cloning upstream pbtkit"
    git clone --depth 1 https://github.com/DRMacIver/pbtkit.git \
        resources/pbtkit
fi

# -----------------------------------------------------------------------------
# 9. Seed the local Claude-Code permission file so agents dispatched by
# port-loop.py can edit .claude/skills/** without permission prompts.
# This file is gitignored, so it doesn't get cloned.
# -----------------------------------------------------------------------------
SETTINGS_LOCAL="$REPO_DIR/.claude/settings.local.json"
if [[ ! -f "$SETTINGS_LOCAL" ]]; then
    log "seeding .claude/settings.local.json"
    mkdir -p "$(dirname "$SETTINGS_LOCAL")"
    cat > "$SETTINGS_LOCAL" <<'JSON'
{
  "permissions": {
    "allow": [
      "Edit(.claude/skills/**)",
      "Write(.claude/skills/**)",
      "Edit(.claude/commands/**)",
      "Write(.claude/commands/**)",
      "Edit(.claude/agents/**)",
      "Write(.claude/agents/**)"
    ]
  }
}
JSON
fi

# -----------------------------------------------------------------------------
# 10. Prime the build cache so the first port-loop iteration doesn't
# cold-compile everything while the supervisor is trying to drive gates.
# -----------------------------------------------------------------------------
log "priming cargo build (warm cache)"
cargo build --tests --quiet || true

# -----------------------------------------------------------------------------
# Done.
# -----------------------------------------------------------------------------
cat <<EOF

[setup] Done. Next steps:

  1. In a fresh shell (so PATH picks up cargo/uv/claude):
       source ~/.bashrc
  2. Authenticate Claude Code if you haven't on this machine:
       claude
     (the first invocation walks you through login).
  3. Start the port loop in tmux:
       tmux new -s port
       cd $REPO_DIR
       ./scripts/port-loop.py --model claude-opus-4-6 \\
           --dangerously-skip-permissions --max-workers 4

  Disk guidance: per-worker target dirs + supervisor target/ + OS
  typically needs ~80 GiB free for --max-workers 4. The script's
  own cleanup will evict idle target dirs when free space drops
  below 20 GiB, but that assumes the disk is big enough to start
  with.
EOF
