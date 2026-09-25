use super::*;
use crate::ffi::{RunHandle, SettingsHandle};
use crate::generators as gs;
use crate::runner::Settings;
use crate::test_case::current_output_sink;

/// Start a real engine run and hand back its first live test case, keeping
/// the owning [`RunHandle`] alive alongside it.
fn live_test_case() -> (RunHandle, TestCase) {
    let settings = Settings::new().database(None);
    let c_settings = SettingsHandle::build(&settings, None, None);
    let run = RunHandle::start(&c_settings, None).unwrap();
    let c_tc = run.next_test_case().unwrap();
    let tc = TestCase::new(Arc::new(c_tc), true, current_output_sink());
    (run, tc)
}

/// A subtree core whose leaf exhausts the stream (catching the unwind, as a
/// test body may) and hands a value straight back, with none of the span
/// bookkeeping a `TestCase` draw would run in between: the finished-value
/// check is the first thing to meet the aborted stream.
struct ExhaustingCore;

impl SubtreeDraw<i64> for ExhaustingCore {
    fn draw_leaf(&self, tc: &TestCase, _printer: &mut PrettyPrinter) -> i64 {
        let exhausted = catch_unwind(AssertUnwindSafe(|| {
            loop {
                tc.draw_silent(gs::integers::<i64>());
            }
        }));
        assert!(exhausted.is_err(), "the draw budget is finite");
        0
    }

    fn draw_branch(
        &self,
        _tc: &TestCase,
        _subtrees: SubtreeGenerator<i64>,
        _printer: &mut PrettyPrinter,
    ) -> i64 {
        unreachable!("a max_depth of 0 never branches")
    }
}

/// The finished-value check must surface an overrun rather than accept the
/// value, even when nothing before it noticed the stream was already gone.
#[test]
fn finishing_a_recursive_value_on_an_exhausted_stream_is_an_overrun() {
    let (_run, tc) = live_test_case();
    let recursion = match tc.with_ctc(|ctc| ctc.new_recursion(0, DEFAULT_MAX_LEAVES as u64)) {
        Ok(recursion) => Arc::new(recursion),
        Err(rc) => raise_for_rc(rc),
    };
    let root = SubtreeGenerator {
        core: Arc::new(ExhaustingCore),
        recursion,
        depth: 0,
        label: RECURSIVE_LABEL,
    };
    let payload = catch_unwind(AssertUnwindSafe(|| {
        root.draw_subtree(&tc, &mut PrettyPrinter::noop())
    }))
    .unwrap_err();
    assert!(
        payload.downcast_ref::<crate::control::StopTest>().is_some(),
        "finishing on an exhausted stream should unwind as StopTest"
    );
}
