use super::*;
use crate::ffi::{RunHandle, SettingsHandle};
use crate::generators as gs;
use crate::runner::Settings;
use crate::test_case::OutputSink;
use std::backtrace::Backtrace;
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};

type Captured = Arc<Mutex<Vec<String>>>;

/// Start a real engine run and hand back its first live test case with an
/// emitting sink that captures every draw/note line, alongside the owning
/// [`RunHandle`].
fn capturing_test_case() -> (RunHandle, TestCase, Captured) {
    let settings = Settings::new().database(None);
    let c_settings = SettingsHandle::build(&settings, None);
    let run = RunHandle::start(&c_settings, None).unwrap();
    let c_tc = run.next_test_case().unwrap();
    let lines: Captured = Arc::default();
    let sink_lines = Arc::clone(&lines);
    let sink: OutputSink = Arc::new(move |msg: &str| {
        sink_lines
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(msg.to_string());
    });
    let tc = TestCase::new(Arc::new(c_tc), true, Mode::TestRun, true, Some(sink));
    (run, tc, lines)
}

/// Register a single-group, concurrency-`concurrency` machine over `rules`
/// on `tc` and return its id.
fn register_machine(tc: &TestCase, rules: &[&str], concurrency: i64) -> i64 {
    let rule_groups = vec![0i64; rules.len()];
    tc.with_ctc(|ctc| {
        ctc.new_state_machine(&[ANONYMOUS_GROUP], rules, &rule_groups, &[], concurrency)
    })
    .unwrap()
}

fn string_panic(message: &str) -> Box<dyn std::any::Any + Send> {
    Box::new(message.to_string())
}

fn panicked_event(message: &str, location: &str, info: Option<PanicInfo>) -> WorkerEvent {
    WorkerEvent::Panicked {
        payload: string_panic(message),
        info,
        location: location.to_string(),
        message: message.to_string(),
    }
}

fn resolve_round_unwind(events: Vec<WorkerEvent>, tc: &TestCase) -> Box<dyn std::any::Any + Send> {
    catch_unwind(AssertUnwindSafe(|| resolve_round(events, tc)))
        .expect_err("the round must classify as terminal")
}

#[test]
fn resolve_round_with_all_workers_done_returns_normally() {
    let (_run, tc, lines) = capturing_test_case();
    resolve_round(vec![WorkerEvent::RoundDone, WorkerEvent::RoundDone], &tc);
    assert!(lines.lock().unwrap().is_empty());
}

#[test]
fn resolve_round_control_payloads_win_over_overrun_and_panic() {
    let (_run, tc, _lines) = capturing_test_case();
    let events = vec![
        WorkerEvent::Overrun,
        WorkerEvent::ControlPayload(Box::new(InternalError("ferried".to_string()))),
        panicked_event("late panic", "a.rs:1:1", None),
    ];
    let payload = resolve_round_unwind(events, &tc);
    let internal = payload.downcast_ref::<InternalError>().unwrap();
    assert_eq!(internal.0, "ferried");
}

#[test]
fn resolve_round_overrun_wins_over_panic_and_notes_the_dropped_panic() {
    let (_run, tc, lines) = capturing_test_case();
    let events = vec![
        panicked_event("induced panic", "b.rs:2:2", None),
        WorkerEvent::Overrun,
    ];
    let payload = resolve_round_unwind(events, &tc);
    assert!(payload.downcast_ref::<StopTest>().is_some());
    let lines = lines.lock().unwrap();
    assert_eq!(lines.len(), 1);
    assert!(
        lines[0].contains("Dropped concurrent panic from worker 0 at b.rs:2:2: induced panic"),
        "{lines:?}"
    );
}

#[test]
fn resolve_round_invalid_wins_over_panic_and_raises_assume_failed() {
    let (_run, tc, lines) = capturing_test_case();
    let events = vec![
        WorkerEvent::Invalid,
        panicked_event("induced panic", "c.rs:3:3", None),
    ];
    let payload = resolve_round_unwind(events, &tc);
    assert!(payload.downcast_ref::<AssumeFailed>().is_some());
    assert_eq!(lines.lock().unwrap().len(), 1);
}

#[test]
fn resolve_round_lowest_worker_index_panic_wins_and_losers_are_noted() {
    let (_run, tc, lines) = capturing_test_case();
    run_lifecycle::take_panic_info();
    let info: PanicInfo = (
        "worker-thread".to_string(),
        "7".to_string(),
        "winner.rs:1:1".to_string(),
        Backtrace::disabled(),
    );
    let events = vec![
        WorkerEvent::RoundDone,
        panicked_event("the winner", "winner.rs:1:1", Some(info)),
        panicked_event("the loser", "loser.rs:9:9", None),
    ];
    let payload = resolve_round_unwind(events, &tc);
    assert_eq!(payload.downcast_ref::<String>().unwrap(), "the winner");
    let (thread_name, _, location, _) = run_lifecycle::take_panic_info().unwrap();
    assert_eq!(thread_name, "worker-thread");
    assert_eq!(location, "winner.rs:1:1");
    let lines = lines.lock().unwrap();
    assert_eq!(lines.len(), 1);
    assert!(
        lines[0].contains("Dropped concurrent panic from worker 2 at loser.rs:9:9: the loser"),
        "{lines:?}"
    );
}

#[test]
fn resolve_round_treats_a_dead_worker_as_an_internal_error() {
    let (_run, tc, _lines) = capturing_test_case();
    let payload = resolve_round_unwind(vec![WorkerEvent::Died], &tc);
    let internal = payload.downcast_ref::<InternalError>().unwrap();
    assert!(internal.0.contains("exited without reporting an outcome"));
}

#[test]
fn classify_worker_unwind_maps_each_control_payload() {
    assert!(matches!(
        classify_worker_unwind(Box::new(AssumeFailed)),
        UnwindClass::Assume
    ));
    assert!(matches!(
        classify_worker_unwind(Box::new(StopTest)),
        UnwindClass::Overrun
    ));
    assert!(matches!(
        classify_worker_unwind(Box::new(InvalidArgument("bad".to_string()))),
        UnwindClass::Control(_)
    ));
    assert!(matches!(
        classify_worker_unwind(Box::new(InternalError("bug".to_string()))),
        UnwindClass::Control(_)
    ));
    assert!(matches!(
        classify_worker_unwind(Box::new(LoopDone)),
        UnwindClass::Control(_)
    ));
}

#[test]
fn terminal_event_maps_every_unwind_class() {
    assert!(matches!(
        terminal_event(Box::new(AssumeFailed)),
        WorkerEvent::Invalid
    ));
    assert!(matches!(
        terminal_event(Box::new(StopTest)),
        WorkerEvent::Overrun
    ));
    assert!(matches!(
        terminal_event(Box::new(InternalError("bug".to_string()))),
        WorkerEvent::ControlPayload(_)
    ));
    run_lifecycle::take_panic_info();
    assert!(matches!(
        terminal_event(string_panic("raw panic")),
        WorkerEvent::Panicked { .. }
    ));
}

#[test]
fn classify_worker_unwind_renders_a_panic_without_captured_info() {
    run_lifecycle::take_panic_info();
    let class = classify_worker_unwind(string_panic("raw panic"));
    let UnwindClass::Panic(WorkerEvent::Panicked {
        info,
        location,
        message,
        ..
    }) = class
    else {
        panic!("expected a panic classification");
    };
    assert!(info.is_none());
    assert_eq!(location, "<unknown>");
    assert_eq!(message, "raw panic");
}

struct HitCounter {
    hits: AtomicI64,
}

impl ConcurrentStateMachine for HitCounter {
    fn rules(&self) -> Vec<ConcurrentRule<Self>> {
        vec![ConcurrentRule::new("hit", ANONYMOUS_GROUP, |m, _tc| {
            m.hits.fetch_add(1, Ordering::SeqCst);
        })]
    }
    fn invariants(&self) -> Vec<ConcurrentInvariant<Self>> {
        Vec::new()
    }
}

struct AlwaysPanics;

impl ConcurrentStateMachine for AlwaysPanics {
    fn rules(&self) -> Vec<ConcurrentRule<Self>> {
        vec![ConcurrentRule::new("boom", ANONYMOUS_GROUP, |_m, _tc| {
            panic!("rule boom")
        })]
    }
    fn invariants(&self) -> Vec<ConcurrentInvariant<Self>> {
        Vec::new()
    }
}

#[test]
fn run_worker_round_executes_the_rounds_rule_and_finishes() {
    let (_run, tc, lines) = capturing_test_case();
    let m = HitCounter {
        hits: AtomicI64::new(0),
    };
    let rules = m.rules();
    let machine_id = register_machine(&tc, &["hit"], 1);
    assert!(
        tc.with_ctc(|ctc| ctc.state_machine_next_group(machine_id))
            .unwrap()
    );
    let event = with_test_context(|| run_worker_round(0, &tc, &m, &rules, machine_id));
    assert!(matches!(event, WorkerEvent::RoundDone));
    assert_eq!(m.hits.load(Ordering::SeqCst), 1);
    assert!(
        lines
            .lock()
            .unwrap()
            .iter()
            .any(|line| line.contains("Rule: hit"))
    );
}

#[test]
fn run_worker_round_ferries_a_rule_panic_with_its_capture() {
    run_lifecycle::init_panic_hook();
    let (_run, tc, _lines) = capturing_test_case();
    let m = AlwaysPanics;
    let rules = m.rules();
    let machine_id = register_machine(&tc, &["boom"], 1);
    assert!(
        tc.with_ctc(|ctc| ctc.state_machine_next_group(machine_id))
            .unwrap()
    );
    let event = with_test_context(|| run_worker_round(0, &tc, &m, &rules, machine_id));
    let WorkerEvent::Panicked {
        info,
        location,
        message,
        ..
    } = event
    else {
        panic!("expected a ferried panic");
    };
    assert_eq!(message, "rule boom");
    assert!(location.contains("stateful_tests.rs"), "{location}");
    assert!(info.is_some());
}

#[test]
fn run_worker_round_reports_an_exhausted_budget_as_overrun() {
    let (_run, tc, _lines) = capturing_test_case();
    let m = HitCounter {
        hits: AtomicI64::new(0),
    };
    let rules = m.rules();
    let machine_id = register_machine(&tc, &["hit"], 1);
    assert!(
        tc.with_ctc(|ctc| ctc.state_machine_next_group(machine_id))
            .unwrap()
    );
    let exhausted = with_test_context(|| {
        catch_unwind(AssertUnwindSafe(|| {
            loop {
                tc.draw_silent(gs::integers::<i64>());
            }
        }))
    })
    .expect_err("the family budget is finite");
    assert!(exhausted.downcast_ref::<StopTest>().is_some());
    let event = with_test_context(|| run_worker_round(0, &tc, &m, &rules, machine_id));
    assert!(matches!(event, WorkerEvent::Overrun));
}

#[test]
fn worker_loop_exits_when_the_event_channel_is_gone() {
    let (_run, tc, _lines) = capturing_test_case();
    let m = HitCounter {
        hits: AtomicI64::new(0),
    };
    let rules = m.rules();
    let machine_id = register_machine(&tc, &["hit"], 1);
    let (round_tx, round_rx) = mpsc::channel();
    let (event_tx, event_rx) = mpsc::channel();
    drop(event_rx);
    round_tx.send(WorkerCommand::RunRound).unwrap();
    std::thread::scope(|scope| {
        let m = &m;
        let rules = &rules;
        scope.spawn(move || worker_loop(0, tc, m, rules, machine_id, false, round_rx, event_tx));
    });
}

#[test]
fn worker_loop_exits_when_the_command_channel_is_gone() {
    let (_run, tc, _lines) = capturing_test_case();
    let m = HitCounter {
        hits: AtomicI64::new(0),
    };
    let rules = m.rules();
    let machine_id = register_machine(&tc, &["hit"], 1);
    let (round_tx, round_rx) = mpsc::channel::<WorkerCommand>();
    let (event_tx, _event_rx) = mpsc::channel();
    drop(round_tx);
    std::thread::scope(|scope| {
        let m = &m;
        let rules = &rules;
        scope.spawn(move || worker_loop(0, tc, m, rules, machine_id, false, round_rx, event_tx));
    });
}
