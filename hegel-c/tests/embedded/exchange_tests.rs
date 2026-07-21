//! Embedded tests for `src/exchange.rs`.

use super::*;

use crate::backend::TestCaseResult;
use crate::native::core::NativeTestCase;
use crate::native::data_source::NativeDataSource;

fn fresh_source() -> BoxedDataSource {
    let ntc = NativeTestCase::for_choices(&[], None, None);
    let (data_source, _handle) = NativeDataSource::new(ntc);
    Box::new(data_source)
}

#[test]
fn drive_hands_each_offered_case_to_run_case_and_returns_the_result() {
    let exchange = CaseExchange::new();
    let fut = async {
        for _ in 0..3 {
            exchange.offer(fresh_source()).await;
        }
        "done"
    };
    let mut seen = 0;
    let out = drive(&exchange, fut, |ds| {
        seen += 1;
        ds.mark_complete(&TestCaseResult::Valid);
    });
    assert_eq!(out, "done");
    assert_eq!(seen, 3);
}

#[test]
fn offer_resumes_only_after_the_driver_polls_again() {
    let exchange = CaseExchange::new();
    let resumed = std::cell::Cell::new(false);
    let fut = async {
        exchange.offer(fresh_source()).await;
        resumed.set(true);
    };
    let mut fut = std::pin::pin!(fut);
    let mut cx = std::task::Context::from_waker(std::task::Waker::noop());
    assert!(fut.as_mut().poll(&mut cx).is_pending());
    assert!(!resumed.get());
    let ds = exchange.take();
    ds.mark_complete(&TestCaseResult::Valid);
    assert!(fut.as_mut().poll(&mut cx).is_ready());
    assert!(resumed.get());
}

#[test]
#[should_panic(expected = "engine suspended without offering a test case")]
fn take_panics_when_nothing_was_offered() {
    CaseExchange::new().take();
}

#[test]
fn default_is_an_empty_exchange() {
    let exchange = CaseExchange::default();
    let fut = async {
        exchange.offer(fresh_source()).await;
        7
    };
    let out = drive(&exchange, fut, |ds| {
        ds.mark_complete(&TestCaseResult::Valid);
    });
    assert_eq!(out, 7);
}

#[test]
fn drive_no_yield_returns_the_value_of_a_non_offering_future() {
    assert_eq!(drive_no_yield(async { 41 + 1 }), 42);
}
