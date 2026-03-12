use crate::test_case::{labels, TestCase};
use ciborium::Value;
use std::marker::PhantomData;
use std::sync::Arc;

/// A bundled schema + parse function for schema-based generation.
///
/// The lifetime `'a` ties the BasicGenerator to the generator that created it.
/// `T: 'a` is required because the parse closure returns `T`.
pub struct BasicGenerator<'a, T> {
    schema: Value,
    parse: Box<dyn Fn(Value) -> T + Send + Sync + 'a>,
    _phantom: PhantomData<fn() -> T>,
}

impl<'a, T: 'a> BasicGenerator<'a, T> {
    pub fn new<F: Fn(Value) -> T + Send + Sync + 'a>(schema: Value, f: F) -> Self {
        BasicGenerator {
            schema,
            parse: Box::new(f),
            _phantom: PhantomData,
        }
    }

    pub fn schema(&self) -> &Value {
        &self.schema
    }

    pub fn parse_raw(&self, raw: Value) -> T {
        (self.parse)(raw)
    }

    /// Generate a value by sending the schema to the server and parsing the response.
    pub fn do_draw(&self, tc: &TestCase) -> T {
        self.parse_raw(super::generate_raw(tc, self.schema()))
    }

    /// Transform the output type by composing a function with the parse.
    pub fn map<U: 'a, F: Fn(T) -> U + Send + Sync + 'a>(self, f: F) -> BasicGenerator<'a, U> {
        let old_parse = self.parse;
        BasicGenerator {
            schema: self.schema,
            parse: Box::new(move |raw| f(old_parse(raw))),
            _phantom: PhantomData,
        }
    }
}

/// The core trait for all generators.
///
/// Generators produce values of type `T` and optionally provide a
/// [`BasicGenerator`] for server-based generation via `as_basic()`.
pub trait Generator<T>: Send + Sync {
    #[doc(hidden)]
    fn do_draw(&self, tc: &TestCase) -> T;

    /// Return a BasicGenerator for schema-based generation, if possible.
    #[doc(hidden)]
    fn as_basic(&self) -> Option<BasicGenerator<'_, T>> {
        None
    }

    /// Transform generated values using a function.
    fn map<U, F>(self, f: F) -> Mapped<T, U, F, Self>
    where
        Self: Sized,
        F: Fn(T) -> U + Send + Sync,
    {
        Mapped {
            source: self,
            f: Arc::new(f),
            _phantom: PhantomData,
        }
    }

    /// Generate a value, then use it to create another generator.
    fn flat_map<U, G, F>(self, f: F) -> FlatMapped<T, U, G, F, Self>
    where
        Self: Sized,
        G: Generator<U>,
        F: Fn(T) -> G + Send + Sync,
    {
        FlatMapped {
            source: self,
            f,
            _phantom: PhantomData,
        }
    }

    fn filter<F>(self, predicate: F) -> Filtered<T, F, Self>
    where
        Self: Sized,
        F: Fn(&T) -> bool + Send + Sync,
    {
        Filtered {
            source: self,
            predicate,
            _phantom: PhantomData,
        }
    }

    /// Convert this generator into a type-erased boxed generator.
    fn boxed<'a>(self) -> BoxedGenerator<'a, T>
    where
        Self: Sized + Send + Sync + 'a,
    {
        BoxedGenerator {
            inner: Arc::new(self),
        }
    }
}

impl<T, G: Generator<T>> Generator<T> for &G {
    fn do_draw(&self, tc: &TestCase) -> T {
        (*self).do_draw(tc)
    }

    fn as_basic(&self) -> Option<BasicGenerator<'_, T>> {
        (*self).as_basic()
    }
}

pub struct Mapped<T, U, F, G> {
    source: G,
    f: Arc<F>,
    _phantom: PhantomData<fn(T) -> U>,
}

impl<T, U, F, G> Generator<U> for Mapped<T, U, F, G>
where
    G: Generator<T>,
    F: Fn(T) -> U + Send + Sync,
{
    fn do_draw(&self, tc: &TestCase) -> U {
        if let Some(basic) = self.as_basic() {
            basic.do_draw(tc)
        } else {
            tc.start_span(labels::MAPPED);
            let result = (self.f)(self.source.do_draw(tc));
            tc.stop_span(false);
            result
        }
    }

    fn as_basic(&self) -> Option<BasicGenerator<'_, U>> {
        let source_basic = self.source.as_basic()?;
        let f = Arc::clone(&self.f);
        Some(source_basic.map(move |t| f(t)))
    }
}

pub struct FlatMapped<T, U, G2, F, G1> {
    source: G1,
    f: F,
    _phantom: PhantomData<fn(T) -> (U, G2)>,
}

impl<T, U, G2, F, G1> Generator<U> for FlatMapped<T, U, G2, F, G1>
where
    G1: Generator<T>,
    G2: Generator<U>,
    F: Fn(T) -> G2 + Send + Sync,
{
    fn do_draw(&self, tc: &TestCase) -> U {
        tc.start_span(labels::FLAT_MAP);
        let intermediate = self.source.do_draw(tc);
        let next_gen = (self.f)(intermediate);
        let result = next_gen.do_draw(tc);
        tc.stop_span(false);
        result
    }
}

pub struct Filtered<T, F, G> {
    source: G,
    predicate: F,
    _phantom: PhantomData<fn() -> T>,
}

impl<T, F, G> Generator<T> for Filtered<T, F, G>
where
    G: Generator<T>,
    F: Fn(&T) -> bool + Send + Sync,
{
    fn do_draw(&self, tc: &TestCase) -> T {
        for _ in 0..3 {
            tc.start_span(labels::FILTER);
            let value = self.source.do_draw(tc);
            if (self.predicate)(&value) {
                tc.stop_span(false);
                return value;
            }
            tc.stop_span(true);
        }
        tc.assume(false);
        unreachable!()
    }
}

/// A type-erased generator with a lifetime parameter.
pub struct BoxedGenerator<'a, T> {
    pub(super) inner: Arc<dyn Generator<T> + Send + Sync + 'a>,
}

impl<T> Clone for BoxedGenerator<'_, T> {
    fn clone(&self) -> Self {
        BoxedGenerator {
            inner: Arc::clone(&self.inner),
        }
    }
}

impl<T> Generator<T> for BoxedGenerator<'_, T> {
    fn do_draw(&self, tc: &TestCase) -> T {
        self.inner.do_draw(tc)
    }

    fn as_basic(&self) -> Option<BasicGenerator<'_, T>> {
        self.inner.as_basic()
    }

    fn boxed<'b>(self) -> BoxedGenerator<'b, T>
    where
        Self: Sized + Send + Sync + 'b,
    {
        BoxedGenerator { inner: self.inner }
    }
}
