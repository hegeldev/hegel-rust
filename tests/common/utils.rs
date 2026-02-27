// internal helper code
#![allow(dead_code)]

use std::sync::{Arc, Mutex};

use hegel::generators::Generate;
use hegel::Hegel;
use std::fmt::Debug;

#[allow(dead_code)]
pub fn check_can_generate_examples<T, G>(generator: G)
where
    G: Generate<T> + 'static,
    T: Debug,
{
    AssertSimpleProperty::new(generator, |_| true).run();
}

pub fn assert_all_examples<T, G, P>(generator: G, predicate: P)
where
    G: Generate<T> + 'static,
    P: Fn(&T) -> bool + 'static,
    T: Debug,
{
    AssertAllExamples::new(generator, predicate).run();
}

#[allow(dead_code)]
pub struct AssertAllExamples<T, G, P>
where
    G: Generate<T> + 'static,
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
    G: Generate<T> + 'static,
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
        Hegel::new(move || {
            let value = hegel::draw(&self.generator);
            assert!(
                (self.predicate)(&value),
                "Found value that does not match predicate"
            );
        })
        .test_cases(self.test_cases)
        .run();
    }
}

#[allow(dead_code)]
pub fn assert_simple_property<T, G, P>(generator: G, predicate: P)
where
    G: Generate<T> + 'static,
    P: Fn(&T) -> bool + 'static,
    T: Debug,
{
    AssertSimpleProperty::new(generator, predicate).run();
}

#[allow(dead_code)]
pub struct AssertSimpleProperty<T, G, P>
where
    G: Generate<T> + 'static,
    P: Fn(&T) -> bool + 'static,
    T: Debug,
{
    inner: AssertAllExamples<T, G, P>,
}

impl<T, G, P> AssertSimpleProperty<T, G, P>
where
    G: Generate<T> + 'static,
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
    G: Generate<T> + 'static,
    P: Fn(&T) -> bool + 'static,
    T: Send + Debug + 'static,
{
    FindAny::new(generator, condition).run()
}

#[allow(dead_code)]
pub struct FindAny<T, G, P>
where
    G: Generate<T> + 'static,
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
    G: Generate<T> + 'static,
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
            Hegel::new(move || {
                let value = hegel::draw(&self.generator);
                if (self.condition)(&value) {
                    *found_clone.lock().unwrap() = Some(value);
                    panic!("HEGEL_FOUND"); // Early exit marker
                }
            })
            .test_cases(max_attempts)
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

#[allow(dead_code)]
pub fn assert_no_examples<T, G, P>(generator: G, condition: P)
where
    G: Generate<T> + 'static,
    P: Fn(&T) -> bool + 'static,
    T: Debug,
{
    AssertNoExamples::new(generator, condition).run();
}

#[allow(dead_code)]
pub struct AssertNoExamples<T, G, P>
where
    G: Generate<T> + 'static,
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
    G: Generate<T> + 'static,
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
