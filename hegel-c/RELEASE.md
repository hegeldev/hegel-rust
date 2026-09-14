RELEASE_TYPE: minor

This release changes the base value of `print_blob` from `false` to `true`, and removes `print_blob = true` from the `ci` profile. 
`hegel_settings_get_print_blob` now returns `true` for a handle from `hegel_settings_new` under `development`, `base`, and `workload`. 
The behavior of `ci` is unchanged. 
