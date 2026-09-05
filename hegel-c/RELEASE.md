RELEASE_TYPE: patch

This patch suppresses `TooSlow` by default in CI, matching [Hypothesis's CI profile](https://github.com/HypothesisWorks/hypothesis/blob/13c3785854da056387aeac789300537e501a3c14/hypothesis-python/src/hypothesis/_settings.py#L759-L772). Calls to `hegel_settings_set_suppress_health_check` still replace the default.
