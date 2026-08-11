//! The suspension point between the engine and whoever drives it.
//!
//! The engine ([`crate::native::test_runner`]) is written as async code with
//! exactly one kind of await point: offering a test case's
//! [`DataSource`](crate::backend::DataSource) to its driver via
//! [`CaseExchange::offer`]. The engine future never schedules wakeups — it is
//! only ever resumed by its driver polling it again — so no executor is
//! involved anywhere: drivers poll with a no-op waker on their own thread,
//! one poll per test case (`hegel_next_test_case`) or in a loop to
//! completion (the test-only `drive`).
//!
//! The protocol generalises strict alternation to a bounded queue. Polling
//! the engine future either returns `Ready` (the run is finished) or
//! `Pending`, in which case the engine has queued zero or more offered cases
//! in the exchange for the driver to take (via
//! [`try_take`](CaseExchange::try_take), or [`take`](CaseExchange::take)
//! where the protocol guarantees one is queued). With a pipeline window of 1
//! (the default) this is exactly the old strict alternation: each `Pending`
//! carries exactly one queued case, and the driver must finish it —
//! everything through `mark_complete` — before polling again. With a wider
//! window the engine may queue several cases before suspending, and a poll
//! may legitimately find some of them still open.

use alloc::boxed::Box;
use alloc::collections::VecDeque;
use core::future::Future;
use core::pin::Pin;
use core::task::{Context, Poll};

use crate::backend::DataSource;
use crate::control::{InternalError, hegel_internal_unwrap};
use crate::sys::sync::Mutex;

/// A data source handed across the exchange, one per test case.
pub(crate) type BoxedDataSource = Box<dyn DataSource + Send + Sync>;

/// The engine-to-driver handoff queue. See the module docs for the protocol.
pub(crate) struct CaseExchange {
    queue: Mutex<VecDeque<BoxedDataSource>>,
}

impl CaseExchange {
    pub(crate) fn new() -> Self {
        CaseExchange {
            queue: Mutex::new(VecDeque::new()),
        }
    }

    /// Queue `ds` for the driver without suspending, so the engine can keep
    /// several cases open at once. The driver picks it up from a later
    /// [`try_take`](Self::take) / [`take`](Self::take).
    pub(crate) fn offer_nowait(&self, ds: BoxedDataSource) {
        self.queue.lock().push_back(ds);
    }

    /// Suspend the engine until the driver polls it again. Combined with
    /// [`offer_nowait`](Self::offer_nowait) this is how the engine hands
    /// control back to the driver; on its own it is how the engine waits for
    /// open cases to conclude.
    pub(crate) fn suspend(&self) -> Yield {
        Yield { suspended: false }
    }

    /// Yield `ds` to the driver: queue it and suspend. With a pipeline
    /// window of 1 this preserves strict alternation — the future resolves
    /// on the next poll, which the driver performs only once the case is
    /// complete.
    pub(crate) async fn offer(&self, ds: BoxedDataSource) {
        self.offer_nowait(ds);
        self.suspend().await;
    }

    /// Take the oldest queued case, or `None` when the queue is empty.
    pub(crate) fn try_take(&self) -> Option<BoxedDataSource> {
        self.queue.lock().pop_front()
    }

    /// Take the case the engine just offered. `Err` if the engine suspended
    /// without offering one, which the alternation protocol rules out — a
    /// bug in the engine, surfaced by the driver as a run-level error
    /// instead of a panic.
    pub(crate) fn take(&self) -> Result<BoxedDataSource, InternalError> {
        let taken = self.try_take();
        Ok(hegel_internal_unwrap!(
            taken,
            "the engine suspended without offering a test case"
        ))
    }
}

impl Default for CaseExchange {
    fn default() -> Self {
        Self::new()
    }
}

/// Future returned by [`CaseExchange::suspend`]: `Pending` on the first
/// poll, `Ready` on the next.
pub(crate) struct Yield {
    suspended: bool,
}

impl Future for Yield {
    type Output = ();

    fn poll(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<()> {
        let this = self.get_mut();
        if this.suspended {
            Poll::Ready(())
        } else {
            this.suspended = true;
            Poll::Pending
        }
    }
}

/// Run an engine future to completion on the calling thread, handing each
/// test case it offers through `exchange` to `run_case`. `run_case` must
/// finish the case — everything through `mark_complete` — before returning,
/// upholding the alternation protocol. Test-only: the C ABI drives the
/// engine future one poll per `hegel_next_test_case` call instead.
#[cfg(test)]
pub(crate) fn drive<F: Future>(
    exchange: &CaseExchange,
    fut: F,
    mut run_case: impl FnMut(BoxedDataSource),
) -> F::Output {
    let mut fut = core::pin::pin!(fut);
    let mut cx = Context::from_waker(core::task::Waker::noop());
    loop {
        match fut.as_mut().poll(&mut cx) {
            Poll::Ready(out) => return out,
            Poll::Pending => run_case(exchange.take().unwrap()),
        }
    }
}

/// Test helper: run a future that must complete without offering any test
/// case — e.g. a shrinker driven by a synchronous probe.
#[cfg(test)]
pub(crate) fn drive_no_yield<F: Future>(fut: F) -> F::Output {
    let mut fut = core::pin::pin!(fut);
    match fut
        .as_mut()
        .poll(&mut Context::from_waker(core::task::Waker::noop()))
    {
        Poll::Ready(out) => out,
        Poll::Pending => unreachable!("future offered a test case but none was expected"),
    }
}

#[cfg(test)]
#[path = "../tests/embedded/exchange_tests.rs"]
mod tests;
