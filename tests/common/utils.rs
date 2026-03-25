// internal helper code
#![allow(dead_code)]

use std::panic::{UnwindSafe, catch_unwind};
use std::sync::{Arc, Mutex};

use hegel::generators::Generator;
use hegel::{Hegel, Settings};
use regex::Regex;
use std::fmt::Debug;

// some of our tests differ in behavior in our nightly rust job.
pub fn is_nightly() -> bool {
    std::env::var("HEGEL_RUNNING_TESTS_WITH_RUST_NIGHTLY").is_ok_and(|v| v == "1")
}

pub fn assert_matches_regex(text: &str, pattern: &str) {
    let re = Regex::new(pattern).unwrap_or_else(|e| panic!("invalid regex {pattern:?}: {e}"));
    assert!(
        re.is_match(text),
        "Expected to match pattern: {pattern}\nActual:\n{text}"
    );
}

/// Run `f` and assert it panics with a message matching the `pattern` regex.
pub fn expect_panic<F: FnOnce() + UnwindSafe>(f: F, pattern: &str) {
    let err = catch_unwind(f).expect_err("expected panic, but closure returned normally");
    let msg = err
        .downcast_ref::<&str>()
        .map(|s| s.to_string())
        .or_else(|| err.downcast_ref::<String>().cloned())
        .unwrap_or_default();
    assert_matches_regex(&msg, pattern);
}

#[allow(dead_code)]
pub fn check_can_generate_examples<T, G>(generator: G)
where
    G: Generator<T> + 'static,
    T: Debug,
{
    AssertSimpleProperty::new(generator, |_| true).run();
}

pub fn assert_all_examples<T, G, P>(generator: G, predicate: P)
where
    G: Generator<T> + 'static,
    P: Fn(&T) -> bool + 'static,
    T: Debug,
{
    AssertAllExamples::new(generator, predicate).run();
}

#[allow(dead_code)]
pub struct AssertAllExamples<T, G, P>
where
    G: Generator<T> + 'static,
    P: Fn(&T) -> bool + 'static,
    T: Debug,
{
    generator: G,
    predicate: P,
    test_cases: u64,
    _marker: std::marker::PhantomData<T>,
}

impl<T, G, P> AssertAllExamples<T, G, P>
where
    G: Generator<T> + 'static,
    P: Fn(&T) -> bool + 'static,
    T: Debug,
{
    pub fn new(generator: G, predicate: P) -> Self {
        Self {
            generator,
            predicate,
            test_cases: 100,
            _marker: std::marker::PhantomData,
        }
    }

    #[allow(dead_code)]
    pub fn test_cases(mut self, n: u64) -> Self {
        self.test_cases = n;
        self
    }

    pub fn run(self) {
        Hegel::new(move |tc| {
            let value = tc.draw(&self.generator);
            assert!(
                (self.predicate)(&value),
                "Found value that does not match predicate"
            );
        })
        .settings(Settings::new().test_cases(self.test_cases))
        .run();
    }
}

#[allow(dead_code)]
pub fn assert_simple_property<T, G, P>(generator: G, predicate: P)
where
    G: Generator<T> + 'static,
    P: Fn(&T) -> bool + 'static,
    T: Debug,
{
    AssertSimpleProperty::new(generator, predicate).run();
}

#[allow(dead_code)]
pub struct AssertSimpleProperty<T, G, P>
where
    G: Generator<T> + 'static,
    P: Fn(&T) -> bool + 'static,
    T: Debug,
{
    inner: AssertAllExamples<T, G, P>,
}

impl<T, G, P> AssertSimpleProperty<T, G, P>
where
    G: Generator<T> + 'static,
    P: Fn(&T) -> bool + 'static,
    T: Debug,
{
    pub fn new(generator: G, predicate: P) -> Self {
        Self {
            inner: AssertAllExamples::new(generator, predicate).test_cases(15),
        }
    }

    #[allow(dead_code)]
    pub fn test_cases(mut self, n: u64) -> Self {
        self.inner = self.inner.test_cases(n);
        self
    }

    pub fn run(self) {
        self.inner.run();
    }
}

pub fn find_any<T, G, P>(generator: G, condition: P) -> T
where
    G: Generator<T> + 'static,
    P: Fn(&T) -> bool + 'static,
    T: Send + Debug + 'static,
{
    FindAny::new(generator, condition).run()
}

#[allow(dead_code)]
pub struct FindAny<T, G, P>
where
    G: Generator<T> + 'static,
    P: Fn(&T) -> bool + 'static,
    T: Send + Debug + 'static,
{
    generator: G,
    condition: P,
    max_attempts: u64,
    _marker: std::marker::PhantomData<T>,
}

impl<T, G, P> FindAny<T, G, P>
where
    G: Generator<T> + 'static,
    P: Fn(&T) -> bool + 'static,
    T: Send + Debug + 'static,
{
    pub fn new(generator: G, condition: P) -> Self {
        Self {
            generator,
            condition,
            max_attempts: 1000,
            _marker: std::marker::PhantomData,
        }
    }

    #[allow(dead_code)]
    pub fn max_attempts(mut self, n: u64) -> Self {
        self.max_attempts = n;
        self
    }

    pub fn run(self) -> T {
        let found: Arc<Mutex<Option<T>>> = Arc::new(Mutex::new(None));
        let found_clone = Arc::clone(&found);
        let max_attempts = self.max_attempts;

        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            Hegel::new(move |tc| {
                let value = tc.draw(&self.generator);
                if (self.condition)(&value) {
                    *found_clone.lock().unwrap() = Some(value);
                    panic!("HEGEL_FOUND"); // Early exit marker
                }
            })
            .settings(Settings::new().test_cases(max_attempts))
            .run();
        }));

        let result = found.lock().unwrap().take();
        result.unwrap_or_else(|| {
            panic!(
                "Could not find any examples satisfying the condition after {} attempts",
                max_attempts
            )
        })
    }
}

/// Find the minimal example from a generator that satisfies the given condition.
///
/// This runs a property test where any value satisfying `condition` causes a failure,
/// then lets Hegel shrink the failing case to find the minimal counterexample.
/// Analogous to Hypothesis's `minimal()` test helper.
#[allow(dead_code)]
pub fn minimal<T, G, P>(generator: G, condition: P) -> T
where
    G: Generator<T> + 'static,
    P: Fn(&T) -> bool + 'static,
    T: Send + Debug + 'static,
{
    Minimal::new(generator, condition).run()
}

#[allow(dead_code)]
pub struct Minimal<T, G, P>
where
    G: Generator<T> + 'static,
    P: Fn(&T) -> bool + 'static,
    T: Send + Debug + 'static,
{
    generator: G,
    condition: P,
    test_cases: u64,
    _marker: std::marker::PhantomData<T>,
}

impl<T, G, P> Minimal<T, G, P>
where
    G: Generator<T> + 'static,
    P: Fn(&T) -> bool + 'static,
    T: Send + Debug + 'static,
{
    pub fn new(generator: G, condition: P) -> Self {
        Self {
            generator,
            condition,
            test_cases: 500,
            _marker: std::marker::PhantomData,
        }
    }

    #[allow(dead_code)]
    pub fn test_cases(mut self, n: u64) -> Self {
        self.test_cases = n;
        self
    }

    pub fn run(self) -> T {
        let found: Arc<Mutex<Option<T>>> = Arc::new(Mutex::new(None));
        let found_clone = Arc::clone(&found);
        let test_cases = self.test_cases;

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            Hegel::new(move |tc| {
                let value = tc.draw(&self.generator);
                if (self.condition)(&value) {
                    *found_clone.lock().unwrap() = Some(value);
                    panic!("HEGEL_MINIMAL_FOUND");
                }
            })
            .settings(
                Settings::new()
                    .test_cases(test_cases)
                    .database(None)
                    .derandomize(true),
            )
            .run();
        }));

        if let Err(payload) = result {
            let msg = payload
                .downcast_ref::<&str>()
                .copied()
                .or_else(|| payload.downcast_ref::<String>().map(|s| s.as_str()));
            let is_expected = msg.is_some_and(|s| s == "Property test failed: HEGEL_MINIMAL_FOUND");
            if !is_expected {
                std::panic::resume_unwind(payload);
            }
        }

        let result = found.lock().unwrap().take();
        result.unwrap_or_else(|| {
            panic!(
                "Could not find any examples satisfying the condition after {} attempts",
                test_cases
            )
        })
    }
}

#[allow(dead_code)]
pub fn assert_no_examples<T, G, P>(generator: G, condition: P)
where
    G: Generator<T> + 'static,
    P: Fn(&T) -> bool + 'static,
    T: Debug,
{
    AssertNoExamples::new(generator, condition).run();
}

#[allow(dead_code)]
pub struct AssertNoExamples<T, G, P>
where
    G: Generator<T> + 'static,
    P: Fn(&T) -> bool + 'static,
    T: Debug,
{
    generator: G,
    condition: P,
    test_cases: u64,
    _marker: std::marker::PhantomData<T>,
}

impl<T, G, P> AssertNoExamples<T, G, P>
where
    G: Generator<T> + 'static,
    P: Fn(&T) -> bool + 'static,
    T: Debug,
{
    pub fn new(generator: G, condition: P) -> Self {
        Self {
            generator,
            condition,
            test_cases: 100,
            _marker: std::marker::PhantomData,
        }
    }

    #[allow(dead_code)]
    pub fn test_cases(mut self, n: u64) -> Self {
        self.test_cases = n;
        self
    }

    pub fn run(self) {
        AssertAllExamples::new(self.generator, move |v| !(self.condition)(v))
            .test_cases(self.test_cases)
            .run();
    }
}
