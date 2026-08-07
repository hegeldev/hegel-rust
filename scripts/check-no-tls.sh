#!/usr/bin/env bash
# Check that a built libhegel shared library registers no thread-local
# state and exports nothing beyond the hegel_* C API:
#   - a dynamic reference to __cxa_thread_atexit_impl, __tls_get_addr, or
#     pthread_key_create means some code path can plant a destructor or
#     key pointing into the library, which dangles after dlclose and
#     crashes the host at thread teardown;
#   - a PT_TLS program header means the library carries initial-exec
#     thread-locals, which pin per-thread state the dynamic loader cannot
#     reclaim on dlclose;
#   - any defined dynamic symbol outside hegel_* (e.g. the unwinder stubs)
#     could be interposed into other shared objects under RTLD_GLOBAL and
#     leave their PLT entries dangling after dlclose.
# Run against the no-std runtime build (`just c-test-runtime`), whose
# whole point is that none of these exist.
set -euo pipefail

lib="${1:?usage: check-no-tls.sh <path-to-libhegel_c.so>}"

symbols=$(nm -D "$lib")
bad=$(echo "$symbols" | grep -E '__cxa_thread_atexit_impl|__tls_get_addr|pthread_key_create' || true)

if [ -n "$bad" ]; then
    echo "error: $lib references thread-local-storage machinery:" >&2
    echo "$bad" >&2
    exit 1
fi

tls_segment=$(readelf -lW "$lib" | awk '$1 == "TLS"')

if [ -n "$tls_segment" ]; then
    echo "error: $lib has a PT_TLS program header:" >&2
    echo "$tls_segment" >&2
    exit 1
fi

# A defined dynamic symbol has an address field, giving the line three
# fields (address, type, name); undefined symbols ("U"/"w") have no
# address. Flagging every defined symbol regardless of type letter fails
# closed for the kinds a whitelist would miss (R/V/i/u — rodata, weak
# objects, ifuncs, GNU-unique).
stray_exports=$(echo "$symbols" | awk 'NF == 3 && $3 !~ /^hegel_/' || true)

if [ -n "$stray_exports" ]; then
    echo "error: $lib exports dynamic symbols outside hegel_*:" >&2
    echo "$stray_exports" >&2
    exit 1
fi

echo "ok: $lib has no TLS machinery and exports only hegel_* symbols"
