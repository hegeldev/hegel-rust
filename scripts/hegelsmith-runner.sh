#!/usr/bin/env bash
# Read a hegel-rust program on stdin, drop it into a pre-built temp Cargo
# project that depends on hegel (with --features native), and run it.
#
# Exit codes:
#   0  — program ran fine, OR ran and panicked with the expected
#        "Property test failed:" message (i.e. found a counterexample to
#        its own random assertion). These are not interesting to hegelsmith.
#   1  — anything else: compile error, unreachable!(), unexpected panic,
#        signal kill. Hegelsmith treats this as a counterexample and shrinks.
#
# The runtime project lives at $HEGELSMITH_RUNTIME (default /tmp/hegelsmith-runtime).
# It must be set up and pre-built once before running this script — see
# scripts/hegelsmith-runner-setup.sh.
set -u

RUNTIME="${HEGELSMITH_RUNTIME:-/tmp/hegelsmith-runtime}"

if [ ! -d "$RUNTIME" ]; then
    echo "runtime project not found at $RUNTIME — run scripts/hegelsmith-runner-setup.sh first" >&2
    exit 2
fi

cat > "$RUNTIME/src/main.rs"

REPO="$(cd "$(dirname "$0")/.." && pwd)"
output=$(cd "$RUNTIME" && CARGO_TARGET_DIR="$REPO/target/hegelsmith-runtime" cargo run --release --quiet 2>&1)
status=$?

if [ "$status" -eq 0 ]; then
    exit 0
fi

# Inner program panicked. The expected case is hegel's own
# "Property test failed:" panic, which means the random assertion in the
# generated body found a counterexample — that is correct hegel behaviour
# and not interesting.
if printf '%s' "$output" | grep -q 'Property test failed:'; then
    exit 0
fi

# Anything else (build error, unreachable!, internal panic, signal) is
# what hegelsmith is hunting for.
printf '%s\n' "$output" >&2
exit 1
