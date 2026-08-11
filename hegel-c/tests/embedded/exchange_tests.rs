//! Embedded tests for `src/exchange.rs`.

use super::*;
use alloc::string::ToString;

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
    let ds = exchange.take().unwrap();
    ds.mark_complete(&TestCaseResult::Valid);
    assert!(fut.as_mut().poll(&mut cx).is_ready());
    assert!(resumed.get());
}

#[test]
fn offer_nowait_queues_without_suspending() {
    let exchange = CaseExchange::new();
    exchange.offer_nowait(fresh_source());
    exchange.offer_nowait(fresh_source());
    assert!(exchange.try_take().is_some());
    assert!(exchange.try_take().is_some());
    assert!(exchange.try_take().is_none());
}

#[test]
fn queued_cases_are_taken_in_offer_order() {
    let exchange = CaseExchange::new();
    let one_choice = NativeTestCase::for_choices(
        &[crate::native::core::ChoiceValue::Boolean(true)],
        None,
        None,
    );
    let (first, _handle) = NativeDataSource::new(one_choice);
    exchange.offer_nowait(Box::new(first));
    exchange.offer_nowait(fresh_source());
    assert!(
        exchange
            .try_take()
            .unwrap()
            .generate_boolean(0.5, None)
            .is_ok()
    );
    assert!(
        exchange
            .try_take()
            .unwrap()
            .generate_boolean(0.5, None)
            .is_err()
    );
}

#[test]
fn suspend_is_pending_once_then_ready() {
    let exchange = CaseExchange::new();
    let fut = exchange.suspend();
    let mut fut = std::pin::pin!(fut);
    let mut cx = std::task::Context::from_waker(std::task::Waker::noop());
    assert!(fut.as_mut().poll(&mut cx).is_pending());
    assert!(fut.as_mut().poll(&mut cx).is_ready());
}

#[test]
fn offer_queues_several_cases_across_one_suspension() {
    let exchange = CaseExchange::new();
    let fut = async {
        exchange.offer_nowait(fresh_source());
        exchange.offer(fresh_source()).await;
    };
    let mut fut = std::pin::pin!(fut);
    let mut cx = std::task::Context::from_waker(std::task::Waker::noop());
    assert!(fut.as_mut().poll(&mut cx).is_pending());
    assert!(exchange.try_take().is_some());
    assert!(exchange.try_take().is_some());
    assert!(exchange.try_take().is_none());
    assert!(fut.as_mut().poll(&mut cx).is_ready());
}

#[test]
fn take_errors_when_nothing_was_offered() {
    let err = CaseExchange::new().take().err().unwrap();
    let msg = err.to_string();
    assert!(
        msg.contains("suspended without offering a test case"),
        "{msg}"
    );
    assert!(msg.contains("bug in hegel"), "{msg}");
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
