//! Tests for the shrinker's run-level error channel: a violated internal
//! invariant (or other [`RunError`]) raised during a shrink run propagates
//! out of [`Shrinker::shrink`] as an `Err` instead of being absorbed like
//! the deadline sentinel.

use super::*;
use crate::control::InternalError;
use crate::exchange::drive_no_yield;
use crate::native::bignum::BigInt;
use crate::native::core::ChoiceNode;
use crate::native::core::choices::IntegerChoice;

fn int_node(value: i128) -> ChoiceNode {
    ChoiceNode::integer(
        IntegerChoice {
            min_value: BigInt::from(0),
            max_value: BigInt::from(1000),
            shrink_towards: BigInt::from(0),
        },
        BigInt::from(value),
        false,
    )
}

struct FailingProbe;

impl ShrinkProbe for FailingProbe {
    fn run<'s>(&'s mut self, _req: ShrinkRun<'s>) -> ProbeFuture<'s> {
        Box::pin(std::future::ready(Err(ShrinkHalt::from(
            InternalError::new(format_args!("probe invariant violated")),
        ))))
    }
}

#[test]
fn internal_error_from_the_probe_surfaces_from_shrink() {
    let mut shrinker =
        Shrinker::with_probe(Box::new(FailingProbe), vec![int_node(5)], Spans::new());
    let err = drive_no_yield(shrinker.shrink()).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("probe invariant violated"), "{msg}");
    assert!(msg.contains("bug in hegel"), "{msg}");
}

#[test]
fn absorb_stop_keeps_run_errors_and_absorbs_the_deadline() {
    assert_eq!(absorb_stop::<()>(Ok(())), Ok(()));
    assert_eq!(absorb_stop::<()>(Err(ShrinkHalt::Stop)), Ok(()));
    let e = InternalError::new(format_args!("halted"));
    assert_eq!(
        absorb_stop::<()>(Err(ShrinkHalt::from(e.clone()))),
        Err(RunError::Internal(e))
    );
    let usage = RunError::UsageError("driver misbehaved".to_string());
    assert_eq!(
        absorb_stop::<()>(Err(ShrinkHalt::from(usage.clone()))),
        Err(usage)
    );
}

#[test]
fn absorb_node_gone_propagates_halts_and_swallows_node_gone() {
    assert_eq!(absorb_node_gone::<()>(Ok(())), Ok(()));
    assert_eq!(absorb_node_gone::<()>(Err(PassExit::NodeGone)), Ok(()));
    assert_eq!(
        absorb_node_gone::<()>(Err(PassExit::from(ShrinkHalt::Stop))),
        Err(ShrinkHalt::Stop)
    );
    let e = InternalError::new(format_args!("halted"));
    assert_eq!(
        absorb_node_gone::<()>(Err(PassExit::from(e.clone()))),
        Err(ShrinkHalt::Error(RunError::Internal(e.clone())))
    );
    assert_eq!(
        ShrinkHalt::from(e.clone()),
        ShrinkHalt::Error(RunError::Internal(e))
    );
}
