#![allow(dead_code)]

mod common;

use std::collections::{HashMap, HashSet};
use std::fmt::Debug;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::{Arc, Mutex};

use hegel::DefaultGenerator as DeriveGenerator;
use hegel::generators::{self as gs, Generator};
use hegel::{HealthCheck, Hegel, Settings, TestCase};

const CASES: u64 = 8;

fn check<T, G, P>(generator: G, property: P)
where
    G: Generator<T> + 'static,
    P: Fn(&T) -> bool + 'static,
    T: Debug,
{
    Hegel::new(move |tc: TestCase| {
        let value = tc.draw(&generator);
        assert!(property(&value), "property failed for {value:?}");
    })
    .settings(
        Settings::new()
            .test_cases(CASES)
            .database(None)
            .suppress_health_check(HealthCheck::all()),
    )
    .run();
}

fn minimal<T, G, P>(generator: G, condition: P) -> T
where
    G: Generator<T> + 'static,
    P: Fn(&T) -> bool + 'static,
    T: Send + Debug + 'static,
{
    let found: Arc<Mutex<Option<T>>> = Arc::new(Mutex::new(None));
    let found_clone = Arc::clone(&found);
    let result = catch_unwind(AssertUnwindSafe(|| {
        Hegel::new(move |tc: TestCase| {
            let value = tc.draw(&generator);
            if condition(&value) {
                *found_clone.lock().unwrap() = Some(value);
                panic!("HEGEL_MINIMAL_FOUND");
            }
        })
        .settings(
            Settings::new()
                .test_cases(50)
                .database(None)
                .derandomize(true)
                .suppress_health_check(HealthCheck::all()),
        )
        .run();
    }));
    if let Err(payload) = result {
        let is_expected = payload
            .downcast_ref::<&str>()
            .copied()
            .or_else(|| payload.downcast_ref::<String>().map(|s| s.as_str()))
            .is_some_and(|s| s == "HEGEL_MINIMAL_FOUND");
        if !is_expected {
            std::panic::resume_unwind(payload);
        }
    }
    found
        .lock()
        .unwrap()
        .take()
        .expect("no value satisfied the condition")
}

#[test]
fn integers_respect_bounds() {
    check(gs::integers::<i64>().min_value(-5).max_value(5), |x| {
        (-5..=5).contains(x)
    });
}

#[test]
fn floats_are_finite_when_bounded() {
    check(gs::floats::<f64>().min_value(0.0).max_value(1.0), |x| {
        (0.0..=1.0).contains(x)
    });
}

#[test]
fn booleans_draw() {
    check(gs::booleans(), |_| true);
}

#[test]
fn text_respects_length() {
    check(gs::text().min_size(1).max_size(4), |s: &String| {
        (1..=4).contains(&s.chars().count())
    });
}

#[test]
fn binary_respects_length() {
    check(gs::binary().min_size(2).max_size(6), |b: &Vec<u8>| {
        (2..=6).contains(&b.len())
    });
}

#[test]
fn vecs_respect_length() {
    check(
        gs::vecs(gs::integers::<i32>()).min_size(1).max_size(5),
        |v: &Vec<i32>| (1..=5).contains(&v.len()),
    );
}

#[test]
fn arrays_have_fixed_length() {
    check(gs::arrays(gs::integers::<i32>()), |a: &[i32; 3]| {
        a.len() == 3
    });
}

#[test]
fn hashmaps_and_hashsets_draw() {
    check(
        gs::hashmaps(gs::integers::<i32>(), gs::booleans()).max_size(4),
        |m: &HashMap<i32, bool>| m.len() <= 4,
    );
    check(
        gs::hashsets(gs::integers::<i32>()).max_size(4),
        |s: &HashSet<i32>| s.len() <= 4,
    );
}

#[test]
fn tuples_draw() {
    check(
        gs::tuples!(gs::integers::<i32>(), gs::booleans(), gs::text()),
        |_| true,
    );
}

#[test]
fn optional_and_one_of_and_sampled() {
    check(gs::optional(gs::integers::<i32>()), |_: &Option<i32>| true);
    check(
        gs::one_of([
            gs::just(1).boxed(),
            gs::just(2).boxed(),
            gs::just(3).boxed(),
        ]),
        |x: &i32| (1..=3).contains(x),
    );
    check(gs::sampled_from(vec!['a', 'b', 'c']), |c: &char| {
        ['a', 'b', 'c'].contains(c)
    });
}

#[test]
fn map_filter_flatmap() {
    check(gs::integers::<i32>().map(|x| x.wrapping_mul(2)), |x| {
        x % 2 == 0
    });
    check(
        gs::integers::<i32>()
            .min_value(0)
            .max_value(20)
            .filter(|x| x % 2 == 0),
        |x| x % 2 == 0,
    );
    let g = gs::integers::<i32>()
        .min_value(0)
        .max_value(3)
        .flat_map(|n| {
            gs::vecs(gs::booleans())
                .min_size(n as usize)
                .max_size(n as usize)
        });
    check(g, |v: &Vec<bool>| v.len() <= 3);
}

#[derive(DeriveGenerator, Debug, Clone)]
struct Point {
    x: i32,
    y: i32,
}

#[derive(DeriveGenerator, Debug, Clone)]
enum Shape {
    Empty,
    Circle(u32),
    Rect { w: u32, h: u32 },
}

#[test]
fn derive_struct_and_enum() {
    check(gs::default::<Point>(), |_| true);
    check(gs::default::<Shape>(), |_| true);
}

#[test]
fn assume_filters_test_cases() {
    Hegel::new(|tc: TestCase| {
        let x: i32 = tc.draw(gs::integers().min_value(0).max_value(10));
        tc.assume(x % 2 == 0);
        assert!(x % 2 == 0);
    })
    .settings(
        Settings::new()
            .test_cases(CASES)
            .database(None)
            .suppress_health_check(HealthCheck::all()),
    )
    .run();
}

#[test]
fn targeting_runs() {
    Hegel::new(|tc: TestCase| {
        let x: i32 = tc.draw(gs::integers().min_value(0).max_value(100));
        tc.target(x as f64);
    })
    .settings(
        Settings::new()
            .test_cases(CASES)
            .database(None)
            .suppress_health_check(HealthCheck::all()),
    )
    .run();
}

#[test]
fn deferred_delegates_to_inner() {
    let d = gs::deferred::<i32>();
    let g = d.generator();
    d.set(gs::integers().min_value(0).max_value(10));
    check(g, |x| (0..=10).contains(x));
}

#[test]
fn shrinks_to_minimal_integer() {
    let x = minimal(gs::integers::<i32>(), |x| *x > 1000);
    assert_eq!(x, 1001);
}

/// A `TestCase` moved into a thread that outlives its test case must stay
/// memory-safe: the body's handle is reference-counted, so the leaked
/// thread's later draw fails by panicking cleanly (the case has finished)
/// instead of touching freed engine state. Run under Miri this catches the
/// use-after-free that a non-owning body handle would reintroduce.
#[test]
fn a_test_case_leaked_to_a_thread_outlives_its_case_safely() {
    use std::sync::mpsc;
    use std::thread::JoinHandle;

    type Leaked = (mpsc::Sender<()>, JoinHandle<()>);
    let worker: Arc<Mutex<Option<Leaked>>> = Arc::new(Mutex::new(None));
    let worker_in_body = Arc::clone(&worker);
    Hegel::new(move |tc: TestCase| {
        let mut slot = worker_in_body.lock().unwrap();
        if slot.is_none() {
            let (release_tx, release_rx) = mpsc::channel();
            let handle = std::thread::spawn(move || {
                release_rx.recv().unwrap();
                tc.draw(gs::booleans());
            });
            *slot = Some((release_tx, handle));
        }
    })
    .settings(
        Settings::new()
            .test_cases(1)
            .database(None)
            .suppress_health_check(HealthCheck::all()),
    )
    .run();

    let (release_tx, handle) = worker.lock().unwrap().take().unwrap();
    release_tx.send(()).unwrap();
    assert!(handle.join().is_err());
}

#[test]
fn output_override_captures_engine_output() {
    let buf: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let buf_writer = Arc::clone(&buf);
    let sink: Arc<dyn Fn(&str) + Send + Sync> =
        Arc::new(move |s: &str| buf_writer.lock().unwrap().push(s.to_string()));
    let result = catch_unwind(AssertUnwindSafe(|| {
        hegel::with_output_override(sink, || {
            Hegel::new(|tc: TestCase| {
                let _ = tc.draw(gs::booleans());
                panic!("always fails");
            })
            .settings(
                Settings::new()
                    .test_cases(2)
                    .database(None)
                    .verbosity(hegel::Verbosity::Debug),
            )
            .run();
        });
    }));
    assert!(result.is_err());
    let lines = buf.lock().unwrap();
    assert!(
        lines.iter().any(|l| l.starts_with("test case #")),
        "expected engine debug lines through the override, got {lines:?}"
    );
}
