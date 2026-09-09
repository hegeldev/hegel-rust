RELEASE_TYPE: minor

This release moves settings defaults into named profiles resolved by the engine, so every language binding shares the same `hegel.toml` configuration and default-profile selection.

Two profile names are reserved. `default` is the immutable base settings. `selected` is an alias for the default profile: the strongest set of `hegel_set_default_profile`, `HEGEL_DEFAULT_PROFILE`, and the `default` entry in `hegel.toml`, else the detected environment (`antithesis` inside Antithesis, `ci` on CI servers), else `development`. Three ordinary profiles ship with the engine: `development` (empty), `ci` (derandomize, database disabled, `too_slow` health check suppressed, print reproduce blobs), and `antithesis` (database disabled, every health check suppressed). A custom profile without an explicit `extends` extends `selected`, skipping candidates already in its chain, so it layers over the environment's profile. The shipped profiles themselves extend `default` and never layer over one another.

- `hegel_settings_new` now resolves the `selected` alias and can fail with `HEGEL_E_INVALID_ARG` when a default-profile setting names an unknown profile or a discovered `hegel.toml` is malformed. Callers must check its return code.
- `HEGEL_CONFIG` names the `hegel.toml` to load directly, replacing the upward search from the working directory, for test processes that run outside the source tree. A set `HEGEL_CONFIG` that cannot be read is an error. The config is loaded once per process, and under debug verbosity each run logs which config file was loaded.
- New functions: `hegel_settings_new_for_profile`, `hegel_settings_register_profile`, `hegel_set_default_profile`, `hegel_settings_set_print_blob`, and a `hegel_settings_get_*` getter for every settings field, so frontends can materialize a resolved profile.
- `hegel_settings_set_database(ctx, settings, NULL)` now resets the database to unset, as its documentation already said, instead of leaving the previous value in place.
- The default for `report_multiple_failures` is now `false`, matching the Rust frontend's documented default.
- The shipped `ci` profile sets `print_blob`, so failing runs on CI print a reproduce blob by default.
- Inside Antithesis, health checks are suppressed by the shipped `antithesis` profile rather than forced off by detection. A `[profiles.antithesis]` delta can set `suppress_health_check`, and resolving a profile that does not extend `antithesis` inside Antithesis runs the health checks.
