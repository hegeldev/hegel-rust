RELEASE_TYPE: patch

This patch adds `hegel_settings_set_max_choices`, which sets the maximum number of choices a single test case may make before the engine concludes it as an overrun, or removes the bound when passed `0`. The default is unchanged at 8192.
