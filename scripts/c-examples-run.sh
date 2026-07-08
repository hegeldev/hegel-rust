#!/usr/bin/env bash
# Compile every C program in hegel-c/examples/ against both the shared
# (libhegel_c.so) and static (libhegel_c.a) builds of libhegel, run each
# binary, and fail loudly if any of them exits non-zero. Exercises the
# linking-mode part of the FFI surface so a static-build-only regression
# (e.g. missing transitive system dep) gets caught in CI.
set -euo pipefail

ROOT=$(cd "$(dirname "$0")/.." && pwd)
INCLUDE="$ROOT/hegel-c/include"
# HEGEL_C_LIB_DIR lets the caller point at a library built into a separate
# target dir — e.g. the panic=abort build produced by `just c-test-abort`.
LIBDIR="${HEGEL_C_LIB_DIR:-$ROOT/target/debug}"
OUT="$ROOT/target/c-examples"
mkdir -p "$OUT"

# System libraries the Rust standard library needs when libhegel is
# linked statically. macOS has no separate librt (its libc absorbs the
# others), so only Linux gets -lrt.
case "$(uname -s)" in
    Darwin)
        STATIC_DEPS=(-lpthread -lm -ldl)
        ;;
    *)
        STATIC_DEPS=(-lpthread -ldl -lm -lrt)
        ;;
esac

CC="${CC:-cc}"
CFLAGS=(-Wall -Wextra -Werror -O0 -g -I"$INCLUDE")

failed=0
for src in "$ROOT"/hegel-c/examples/*.c; do
    name=$(basename "$src" .c)
    shared="$OUT/${name}-shared"
    static="$OUT/${name}-static"

    echo "=== building ${name} (shared) ==="
    "$CC" "${CFLAGS[@]}" -o "$shared" "$src" \
        -L"$LIBDIR" -lhegel_c \
        -Wl,-rpath,"$LIBDIR"

    echo "=== building ${name} (static) ==="
    "$CC" "${CFLAGS[@]}" -o "$static" "$src" \
        "$LIBDIR/libhegel_c.a" "${STATIC_DEPS[@]}"

    echo "=== running ${name} (shared) ==="
    if ! LD_LIBRARY_PATH="$LIBDIR" "$shared"; then
        echo "FAIL: ${name} (shared) exited non-zero" >&2
        failed=1
    fi

    echo "=== running ${name} (static) ==="
    if ! "$static"; then
        echo "FAIL: ${name} (static) exited non-zero" >&2
        failed=1
    fi
done

exit "$failed"
