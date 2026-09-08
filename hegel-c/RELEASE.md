RELEASE_TYPE: patch

This patch adds `hegel_settings_set_max_choices`, which sets the maximum number of choices a single test case may make before the engine concludes it as an overrun, or removes the bound when passed `0`. The default is unchanged at 8192.

The per-round continue draw of stateful test cases now stops with probability 2^-32 per round instead of 2^-16, so a machine with a large step count is no longer cut short by a random stop after tens of thousands of rounds. The step count remains the way to bound a stateful case; the draw only exists so the shrinker can truncate a case at a round boundary.
