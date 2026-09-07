RELEASE_TYPE: patch

This patch makes `serde_json` an optional dependency again, enabled by the `serde_json` feature ([#442](https://github.com/hegeldev/hegel-rust/issues/442)). It had become unconditional when the Antithesis integration stopped being feature-gated. Having `serde_json` in the crate graph changes type inference in downstream tests: its `impl PartialEq<serde_json::Value>` impls for `bool`, integers, and `f64` make comparisons like `assert_eq!(true, Deserialize::deserialize(..).unwrap())` ambiguous.
