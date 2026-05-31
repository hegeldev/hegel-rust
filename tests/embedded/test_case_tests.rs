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
