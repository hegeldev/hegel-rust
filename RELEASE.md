RELEASE_TYPE: patch

Internal change to the native backend (`--features native`): the TooSlow health-check threshold is now passed into the engine rather than read from a constant, so it can be tested deterministically. No user-visible behaviour change.
