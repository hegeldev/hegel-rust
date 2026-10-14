RELEASE_TYPE: minor

Grammar import callbacks now return `hegel_result_t`: `HEGEL_OK` supplies a grammar, the new `HEGEL_E_NOT_FOUND` permits bundled fallback, and other results abort compilation. Callbacks can supply an error diagnostic through `out_error`. This changes the callback ABI.
