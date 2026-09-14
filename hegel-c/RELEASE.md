RELEASE_TYPE: patch

This patch moves the [Antithesis](https://antithesis.com/) integration into libhegel, so every language binding gets it rather than each reimplementing it.

The new `hegel_settings_set_test_location` records where the test under a settings handle lives: its source file and line, the class, module or package enclosing it, and the function name.

```c
hegel_settings_set_test_location(ctx, settings, "tests/list_tests.c", 42, "list_tests", "reversal_is_involutive");
```

Inside Antithesis (detected via `ANTITHESIS_OUTPUT_DIR`), libhegel then writes the verdict of every run started from those settings, and of every test case replayed from a blob with them, to `sdk.jsonl` in the output directory as an `always` assertion in the format Antithesis's SDKs use — identified as `<class_name>::<function> passes properties` — so the property is listed alongside the assertions in the system under test and flagged when it fails. A run that ends in a run-level error (a failed health check, say) is reported as a failure, since it is no verdict on the property. Outside Antithesis, and for settings without a location, nothing is written. Like the database key, the location is per-test identity rather than a setting, and `hegel_settings_register_profile` does not snapshot it.
