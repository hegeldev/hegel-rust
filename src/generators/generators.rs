use super::{labels, TestCaseData};
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
    ///
    /// This is a convenience for `self.parse_raw(data.generate_raw(self.schema()))`.
    pub fn do_draw(&self, data: &TestCaseData) -> T {
        self.parse_raw(data.generate_raw(self.schema()))
    }

    /// Transform the output type by composing a function with the parse.
    ///
    /// The resulting BasicGenerator shares the same schema but applies `f`
    /// after parsing.
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
    fn do_draw(&self, data: &TestCaseData) -> T;

    /// Return a BasicGenerator for schema-based generation, if possible.
    ///
    /// When available, this enables single-request schema-based generation
    /// and allows combinators to compose schemas.
    ///
    /// Returns `None` for generators that cannot be expressed as a schema
    /// (e.g., after `flat_map` or `filter`).
    #[doc(hidden)]
    fn as_basic(&self) -> Option<BasicGenerator<'_, T>> {
        None
    }

    /// Transform generated values using a function.
    ///
    /// If this generator is basic, the resulting generator is also basic
    /// with a composed transform (preserving the schema).
    /// If this generator is not basic, falls back to a MappedGenerator
    /// with span tracking.
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
    ///
    /// This is useful for dependent generation where the second value
    /// depends on the first.
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
    ///
    /// This is useful when you need to store generators of different concrete
    /// types in a collection or struct field.
    fn boxed(self) -> BoxedGenerator<T>
    where
        Self: Sized + 'static,
    {
        BoxedGenerator {
            inner: Arc::new(self),
        }
    }
}

impl<T, G: Generator<T>> Generator<T> for &G {
    fn do_draw(&self, data: &TestCaseData) -> T {
        (*self).do_draw(data)
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
    fn do_draw(&self, data: &TestCaseData) -> U {
        if let Some(basic) = self.as_basic() {
            basic.do_draw(data)
        } else {
            data.start_span(labels::MAPPED);
            let result = (self.f)(self.source.do_draw(data));
            data.stop_span(false);
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
    fn do_draw(&self, data: &TestCaseData) -> U {
        data.start_span(labels::FLAT_MAP);
        let intermediate = self.source.do_draw(data);
        let next_gen = (self.f)(intermediate);
        let result = next_gen.do_draw(data);
        data.stop_span(false);
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
    fn do_draw(&self, data: &TestCaseData) -> T {
        for _ in 0..3 {
            data.start_span(labels::FILTER);
            let value = self.source.do_draw(data);
            if (self.predicate)(&value) {
                data.stop_span(false);
                return value;
            }
            data.stop_span(true);
        }
        crate::assume(false);
        unreachable!()
    }
}

/// A type-erased generator.
///
/// This is useful for storing generators of different concrete types
/// in collections or struct fields.
///
/// Create a `BoxedGenerator` by calling `.boxed()` on any generator.
///
/// # Example
///
/// ```no_run
/// use hegel::generators::{self, Generator, BoxedGenerator};
///
/// fn positive_integers() -> BoxedGenerator<i32> {
///     generators::integers().min_value(1).boxed()
/// }
/// ```
pub struct BoxedGenerator<T> {
    pub(super) inner: Arc<dyn Generator<T> + Send + Sync>,
}

impl<T> Clone for BoxedGenerator<T> {
    fn clone(&self) -> Self {
        BoxedGenerator {
            inner: Arc::clone(&self.inner),
        }
    }
}

impl<T> Generator<T> for BoxedGenerator<T> {
    fn do_draw(&self, data: &TestCaseData) -> T {
        self.inner.do_draw(data)
    }

    fn as_basic(&self) -> Option<BasicGenerator<'_, T>> {
        self.inner.as_basic()
    }

    /// Returns self without re-wrapping.
    fn boxed(self) -> BoxedGenerator<T>
    where
        Self: Sized + 'static,
    {
        self
    }
}
