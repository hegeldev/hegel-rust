RELEASE_TYPE: minor

This release moves settings defaults into named profiles resolved by the engine, so every language binding shares the same `default`/`ci`/`antithesis` selection and the same `hegel.toml` configuration.

- `hegel_settings_new` now resolves the automatically selected profile (`HEGEL_DEFAULT_PROFILE`, then Antithesis/CI detection) and can fail with `HEGEL_E_INVALID_ARG` when that variable names an unknown profile or a discovered `hegel.toml` is malformed. Callers must check its return code.
- `HEGEL_CONFIG` names the `hegel.toml` to load directly, replacing the upward search from the working directory, for test processes that run outside the source tree. A set `HEGEL_CONFIG` that cannot be read is an error. Under debug verbosity each run logs which config file was loaded.
- New functions: `hegel_settings_new_for_profile`, `hegel_settings_register_profile`, `hegel_settings_set_print_blob`, and a `hegel_settings_get_*` getter for every settings field, so frontends can materialize a resolved profile.
- `hegel_settings_set_database(ctx, settings, NULL)` now resets the database to unset, as its documentation already said, instead of leaving the previous value in place.
- The default for `report_multiple_failures` is now `false`, matching the Rust frontend's documented default.
- The shipped `ci` profile sets `print_blob`, so failing runs on CI print a reproduce blob by default.
