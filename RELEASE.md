RELEASE_TYPE: patch

This patch moves Hegel's data-generation engine out of the `hegeltest` crate and into libhegel (the `hegel-c` crate). `hegeltest` now drives the engine entirely through libhegel's C ABI, exactly like the other language bindings. For Rust users this is an internal change — the public API is unchanged — but it keeps us honest: hegel-rust can no longer accidentally depend on engine internals that aren't exposed through the C ABI.

For libhegel C-ABI consumers (such as hegel-go) it adds and hardens some surface:

- A new `hegel_settings_backend(settings, backend)` together with the `hegel_backend_t` enum (`HEGEL_BACKEND_AUTO` / `HEGEL_BACKEND_DEFAULT` / `HEGEL_BACKEND_URANDOM`) lets you choose the engine's source of randomness. `HEGEL_BACKEND_AUTO` is the default: it picks `URANDOM` automatically when running under Antithesis and the seeded, reproducible PRNG (`DEFAULT`) otherwise. This auto-detection now lives in the engine rather than in each binding.

- libhegel no longer panics on invalid arguments, so it is now correct to build it with `panic = "abort"`. `hegel_target` returns `HEGEL_E_INVALID_ARG` (with a message in `hegel_last_error_message`) for a non-finite score or a duplicate label, and the id-taking primitives (`hegel_collection_more`, `hegel_collection_reject`, `hegel_pool_add`, `hegel_pool_generate`, `hegel_state_machine_next_rule`) return `HEGEL_E_INVALID_ARG` for an unknown id instead of aborting the process. Only genuine internal-invariant violations still panic.
