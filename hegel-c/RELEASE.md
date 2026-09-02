RELEASE_TYPE: patch

This patch adds event statistics: `hegel_event` and `hegel_event_value` record labelled observations on the current test case, and with the new `hegel_settings_set_show_statistics` setting the engine prints a statistics block on the run's output at the end of the run — per label, the fraction of generation-phase test cases the event occurred in, and a distribution summary of numeric observations.
