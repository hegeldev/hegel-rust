RELEASE_TYPE: patch

This patch changes the defaults `hegel_settings_new` picks when running inside Antithesis. The failure database is disabled and every health check is skipped. The notice that a concurrent state machine has made the run nondeterministic is also no longer printed inside Antithesis.
