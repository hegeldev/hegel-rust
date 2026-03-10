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
pub trait Generate<T>: Send + Sync {
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
        G: Generate<U>,
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
    ///
    /// The lifetime parameter is inferred from the generator being boxed.
    /// For generators that own all their data, this will be `'static`.
    /// For generators that borrow data, the lifetime will match the borrow.
    fn boxed<'a>(self) -> BoxedGenerator<'a, T>
    where
        Self: Sized + Send + Sync + 'a,
    {
        BoxedGenerator {
            inner: Arc::new(self),
        }
    }
}

impl<T, G: Generate<T>> Generate<T> for &G {
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

impl<T, U, F, G> Generate<U> for Mapped<T, U, F, G>
where
    G: Generate<T>,
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

impl<T, U, G2, F, G1> Generate<U> for FlatMapped<T, U, G2, F, G1>
where
    G1: Generate<T>,
    G2: Generate<U>,
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

impl<T, F, G> Generate<T> for Filtered<T, F, G>
where
    G: Generate<T>,
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

/// A type-erased generator with a lifetime parameter.
///
/// This is useful for storing generators of different concrete types
/// in collections or struct fields.
///
/// Create a `BoxedGenerator` by calling `.boxed()` on any generator.
///
/// The lifetime `'a` represents the minimum lifetime of any borrowed data
/// in the generator. Use `'static` for generators that own all their data.
/// For generators that borrow data, the lifetime will match the borrow.
pub struct BoxedGenerator<'a, T> {
    pub(super) inner: Arc<dyn Generate<T> + Send + Sync + 'a>,
}

impl<T> Clone for BoxedGenerator<'_, T> {
    fn clone(&self) -> Self {
        BoxedGenerator {
            inner: Arc::clone(&self.inner),
        }
    }
}

impl<T> Generate<T> for BoxedGenerator<'_, T> {
    fn do_draw(&self, data: &TestCaseData) -> T {
        self.inner.do_draw(data)
    }

    fn as_basic(&self) -> Option<BasicGenerator<'_, T>> {
        self.inner.as_basic()
    }

    /// Returns self without re-wrapping.
    fn boxed<'b>(self) -> BoxedGenerator<'b, T>
    where
        Self: Sized + Send + Sync + 'b,
    {
        BoxedGenerator { inner: self.inner }
    }
}
