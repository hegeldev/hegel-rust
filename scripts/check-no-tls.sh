#!/usr/bin/env bash
# Check that a built libhegel shared library registers no thread-local
# state: a dynamic reference to __cxa_thread_atexit_impl, __tls_get_addr,
# or pthread_key_create means some code path can plant a destructor or
# key pointing into the library, which dangles after dlclose and crashes
# the host at thread teardown. Run against the no-std runtime build
# (`just c-test-runtime`), whose whole point is that none of these exist.
set -euo pipefail

lib="${1:?usage: check-no-tls.sh <path-to-libhegel_c.so>}"

symbols=$(nm -D "$lib")
bad=$(echo "$symbols" | grep -E '__cxa_thread_atexit_impl|__tls_get_addr|pthread_key_create' || true)

if [ -n "$bad" ]; then
    echo "error: $lib references thread-local-storage machinery:" >&2
    echo "$bad" >&2
    exit 1
fi

echo "ok: $lib has no TLS-registration symbols"
