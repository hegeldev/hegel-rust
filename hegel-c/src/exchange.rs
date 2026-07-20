//! The suspension point between the engine and whoever drives it.
//!
//! The engine ([`crate::native::test_runner`]) is written as async code with
//! exactly one kind of await point: offering a test case's
//! [`DataSource`](crate::backend::DataSource) to its driver via
//! [`CaseExchange::offer`]. The engine future never schedules wakeups — it is
//! only ever resumed by its driver polling it again — so no executor is
//! involved anywhere: [`drive`] runs a whole engine future to completion on
//! the calling thread with a no-op waker.
//!
//! The protocol is strict alternation. Polling the engine future either
//! returns `Ready` (the run is finished) or `Pending`, in which case the
//! engine has stored exactly one offered case in the exchange for the driver
//! to [`take`](CaseExchange::take). The driver must finish that case —
//! everything through `mark_complete` — before polling again: the next poll
//! resumes the engine, which immediately reads the case's outcome off its
//! handle.

use std::future::Future;
use std::pin::Pin;
use std::sync::Mutex;
use std::task::{Context, Poll, Waker};

use crate::backend::DataSource;

/// A data source handed across the exchange, one per test case.
pub(crate) type BoxedDataSource = Box<dyn DataSource + Send + Sync>;

/// One engine-to-driver handoff slot. See the module docs for the protocol.
pub(crate) struct CaseExchange {
    slot: Mutex<Option<BoxedDataSource>>,
}

impl CaseExchange {
    pub(crate) fn new() -> Self {
        CaseExchange {
            slot: Mutex::new(None),
        }
    }

    /// Yield `ds` to the driver. The returned future stores `ds` in the
    /// exchange and suspends; by the alternation protocol it resolves on the
    /// next poll, which the driver performs only once the case is complete.
    pub(crate) fn offer(&self, ds: BoxedDataSource) -> Offer<'_> {
        Offer {
            exchange: self,
            ds: Some(ds),
        }
    }

    /// Take the case the engine just offered. Panics if the engine suspended
    /// without offering one, which the alternation protocol rules out.
    pub(crate) fn take(&self) -> BoxedDataSource {
        self.slot
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take()
            .expect("engine suspended without offering a test case")
    }
}

impl Default for CaseExchange {
    fn default() -> Self {
        Self::new()
    }
}

/// Future returned by [`CaseExchange::offer`]: stores the data source and
/// returns `Pending` on the first poll, `Ready` on the next.
pub(crate) struct Offer<'a> {
    exchange: &'a CaseExchange,
    ds: Option<BoxedDataSource>,
}

impl Future for Offer<'_> {
    type Output = ();

    fn poll(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<()> {
        let this = self.get_mut();
        match this.ds.take() {
            Some(ds) => {
                *this.exchange.slot.lock().unwrap_or_else(|e| e.into_inner()) = Some(ds);
                Poll::Pending
            }
            None => Poll::Ready(()),
        }
    }
}

/// Run an engine future to completion on the calling thread, handing each
/// test case it offers through `exchange` to `run_case`. `run_case` must
/// finish the case — everything through `mark_complete` — before returning,
/// upholding the alternation protocol.
pub(crate) fn drive<F: Future>(
    exchange: &CaseExchange,
    fut: F,
    mut run_case: impl FnMut(BoxedDataSource),
) -> F::Output {
    let mut fut = std::pin::pin!(fut);
    let mut cx = Context::from_waker(Waker::noop());
    loop {
        match fut.as_mut().poll(&mut cx) {
            Poll::Ready(out) => return out,
            Poll::Pending => run_case(exchange.take()),
        }
    }
}

/// Test helper: run a future that must complete without offering any test
/// case — e.g. a shrinker driven by a synchronous probe.
#[cfg(test)]
pub(crate) fn drive_no_yield<F: Future>(fut: F) -> F::Output {
    let mut fut = std::pin::pin!(fut);
    match fut.as_mut().poll(&mut Context::from_waker(Waker::noop())) {
        Poll::Ready(out) => out,
        Poll::Pending => unreachable!("future offered a test case but none was expected"),
    }
}

#[cfg(test)]
#[path = "../tests/embedded/exchange_tests.rs"]
mod tests;
