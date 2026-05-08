#!/usr/bin/env bash
# One-time setup for scripts/hegelsmith-runner.sh: create a temp Cargo
# project that depends on this hegel-rust checkout (path-dep, --features
# native) and pre-build it so subsequent per-program builds only relink
# the bin.
set -euo pipefail

REPO="$(git rev-parse --show-toplevel)"
RUNTIME="${HEGELSMITH_RUNTIME:-/tmp/hegelsmith-runtime}"

mkdir -p "$RUNTIME/src"

cat > "$RUNTIME/Cargo.toml" <<EOF
[package]
name = "hegelsmith_runtime"
version = "0.0.0"
edition = "2024"
publish = false

[dependencies]
hegeltest = { path = "$REPO", features = ["native"] }

[[bin]]
name = "hegelsmith_runtime"
path = "src/main.rs"
EOF

cat > "$RUNTIME/src/main.rs" <<'EOF'
fn main() {}
EOF

# Reuse the workspace target directory so we don't double-compile hegel
# and its deps. Each per-program build then only relinks the runtime bin.
echo "warming up runtime project at $RUNTIME ..." >&2
(cd "$RUNTIME" && CARGO_TARGET_DIR="$REPO/target/hegelsmith-runtime" cargo build --release --quiet)
echo "done." >&2
