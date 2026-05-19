RELEASE_TYPE: patch

The native backend's integer sampler now uses a piecewise log-Student's-t distribution instead of a uniform fallback. The distribution is bell-shaped in `log_2(|x|)` so integer magnitudes spread smoothly across many decades, rather than concentrating at the high-magnitude end of the requested range. This mirrors [HypothesisWorks/hypothesis#4728](https://github.com/HypothesisWorks/hypothesis/pull/4728).
