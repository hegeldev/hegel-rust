RELEASE_TYPE: patch

This patch updates the `dashu-int` dependency from 0.4.1 to 0.6.1, picking up correctness fixes and speedups in the arbitrary-precision integer backend behind the shortlex index arithmetic. The `IBig`/`UBig` API surface the engine uses is unchanged; the dependency also drops its `rustversion` build-time dependency.
