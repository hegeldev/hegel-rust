use crate::common::utils::printed_draw_lines;
use hegel::extras::rand as rand_gs;
use hegel::generators as gs;
use hegel::{Hegel, Settings};
use rand::RngExt;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::{Arc, Mutex};

fn failing_lines<F>(body: F) -> Vec<String>
where
    F: FnMut(hegel::TestCase) + 'static,
{
    let buf: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let writer = buf.clone();
    let sink: Arc<dyn Fn(&str) + Send + Sync> =
        Arc::new(move |s: &str| writer.lock().unwrap().push(s.to_string()));
    let result = catch_unwind(AssertUnwindSafe(|| {
        hegel::with_output_override(sink, || {
            Hegel::new(body)
                .settings(
                    Settings::new()
                        .test_cases(20)
                        .database(None)
                        .derandomize(true)
                        .phases([
                            hegel::Phase::Explicit,
                            hegel::Phase::Reuse,
                            hegel::Phase::Generate,
                            hegel::Phase::Target,
                            hegel::Phase::Shrink,
                        ]),
                )
                .run();
        });
    }));
    assert!(result.is_err(), "expected the property to fail");
    let mut lines = buf.lock().unwrap().clone();
    if let Some(pos) = lines
        .iter()
        .position(|l| l.starts_with("thread '") && l.contains("panicked at"))
    {
        lines.truncate(pos);
    }
    lines
}

#[test]
fn drawn_rngs_print_the_values_they_hand_out() {
    let lines = failing_lines(|tc| {
        let mut rng = tc.draw(rand_gs::randoms());
        let a: u32 = rng.random();
        let b: u64 = rng.random();
        let _ = (a, b);
        panic!("boom");
    });
    assert_eq!(
        lines,
        vec!["let draw_1 = HegelRandom { consumed: [0, 0] };"]
    );
}

#[test]
fn drawn_rngs_record_filled_byte_buffers() {
    let lines = failing_lines(|tc| {
        use rand::rand_core::TryRng;
        let mut rng = tc.draw(rand_gs::randoms());
        let mut buffer = [0u8; 2];
        rng.try_fill_bytes(&mut buffer).unwrap();
        panic!("boom");
    });
    assert_eq!(
        lines,
        vec!["let draw_1 = HegelRandom { consumed: [[0, 0]] };"]
    );
}

#[test]
fn unused_rngs_print_an_empty_record() {
    let lines = failing_lines(|tc| {
        let _ = tc.draw(rand_gs::randoms());
        panic!("boom");
    });
    assert_eq!(lines, vec!["let draw_1 = HegelRandom { consumed: [] };"]);
}

#[test]
fn true_random_rngs_print_their_seed() {
    let lines = failing_lines(|tc| {
        let mut rng = tc.draw(rand_gs::randoms().use_true_random(true));
        let _: u32 = rng.random();
        panic!("boom");
    });
    assert_eq!(lines, vec!["let draw_1 = HegelRandom { seed: 0 };"]);
}

#[test]
fn silently_drawn_rngs_record_nothing() {
    printed_draw_lines(rand_gs::randoms());
    let lines = failing_lines(|tc| {
        let mut rng = tc.draw_silent(rand_gs::randoms());
        let _: u32 = rng.random();
        let _ = tc.draw(gs::booleans());
        panic!("boom");
    });
    assert_eq!(lines, vec!["let draw_1 = false;"]);
}
