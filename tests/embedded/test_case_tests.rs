use super::*;

/// A `DataSource` stub for unit-testing `TestCase`'s draw-output bookkeeping.
///
/// `record_named_draw` only mutates the draw-tracking state and writes to the
/// output sink — it never calls back into the backend — so none of these
/// methods are reached by the tests below.
struct StubDataSource;

impl DataSource for StubDataSource {
    fn generate(&self, _schema: &Value) -> Result<Value, DataSourceError> {
        unimplemented!()
    }
    fn start_span(&self, _label: u64) -> Result<(), DataSourceError> {
        unimplemented!()
    }
    fn stop_span(&self, _discard: bool) -> Result<(), DataSourceError> {
        unimplemented!()
    }
    fn new_collection(
        &self,
        _min_size: u64,
        _max_size: Option<u64>,
    ) -> Result<i64, DataSourceError> {
        unimplemented!()
    }
    fn collection_more(&self, _collection_id: i64) -> Result<bool, DataSourceError> {
        unimplemented!()
    }
    fn collection_reject(
        &self,
        _collection_id: i64,
        _why: Option<&str>,
    ) -> Result<(), DataSourceError> {
        unimplemented!()
    }
    fn primitive_boolean(&self, _p: f64, _forced: Option<bool>) -> Result<bool, DataSourceError> {
        unimplemented!()
    }
    fn new_pool(&self) -> Result<i64, DataSourceError> {
        unimplemented!()
    }
    fn pool_add(&self, _pool_id: i64) -> Result<i64, DataSourceError> {
        unimplemented!()
    }
    fn pool_generate(&self, _pool_id: i64, _consume: bool) -> Result<i64, DataSourceError> {
        unimplemented!()
    }
    fn target_observation(&self, _score: f64, _label: &str) {
        unimplemented!()
    }
    fn mark_complete(&self, _result: &crate::backend::TestCaseResult) {
        unimplemented!()
    }
}

/// Build a `TestCase` whose draw output is emitted (`is_last_run = true`), so
/// the display-name bookkeeping in `record_named_draw` runs — the same path a
/// failing test's final replay takes.
fn emitting_test_case() -> TestCase {
    TestCase::new(Box::new(StubDataSource), true, Mode::TestRun, false)
}

#[test]
fn debug_is_non_exhaustive() {
    let tc = emitting_test_case();
    assert_eq!(format!("{:?}", tc), "TestCase { .. }");
}

#[test]
fn repeatable_display_name_skips_a_taken_name() {
    let tc = emitting_test_case();
    // A non-repeatable draw named "x_1" claims the display name "x_1".
    tc.record_named_draw(&false, "x_1", false);
    // Two repeatable draws named "x" want "x_1" then "x_2"; the first collides
    // with the explicit "x_1" above and must advance the counter, so they end
    // up as "x_2" and "x_3".
    tc.record_named_draw(&false, "x", true);
    tc.record_named_draw(&false, "x", true);

    let mut names: Vec<String> = tc.with_shared(|shared| {
        shared
            .draw_state
            .allocated_display_names
            .iter()
            .cloned()
            .collect()
    });
    names.sort();
    assert_eq!(names, vec!["x_1", "x_2", "x_3"]);
}

/// Recover a panic payload's message as a `String`.
fn panic_payload_message(err: Box<dyn std::any::Any + Send>) -> String {
    err.downcast_ref::<String>()
        .cloned()
        .or_else(|| err.downcast_ref::<&str>().map(|s| s.to_string()))
        .unwrap_or_default()
}

/// An `InvalidArgument` error is raised as a usage error: it carries the
/// diagnostic but no internal marker (the helper is called outside a test
/// context here).
#[test]
fn invalid_argument_error_is_raised_as_a_usage_error() {
    let err = std::panic::catch_unwind(|| {
        panic_on_data_source_error(DataSourceError::InvalidArgument(
            "bad generator configuration".to_string(),
        ))
    })
    .unwrap_err();
    let msg = panic_payload_message(err);
    assert!(msg.contains("bad generator configuration"), "{msg}");
    assert!(!msg.contains("__HEGEL"), "marker leaked: {msg}");
}
