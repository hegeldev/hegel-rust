RELEASE_TYPE: minor

This release adds a new `hegel::embed::run_native` low-level entry point (available with `--features native`), which drives the in-process engine and hands each test case's raw [`backend::DataSource`](crate::backend::DataSource) to a caller-supplied closure. Unlike `Hegel::run`, it does not wrap data sources in a `TestCase` or catch panics — it's intended for FFI, alternative test harnesses, and other low-level drivers that have their own outcome-propagation story.

The `backend::TestRunner::run` trait method's `run_case` parameter type tightens from `Box<dyn DataSource>` to `Box<dyn DataSource + Send + Sync>`. The trait already required `Send + Sync` as supertraits on `DataSource`, so every existing implementation already satisfies the bound; the trait object now reflects it so the data source can cross thread boundaries.
