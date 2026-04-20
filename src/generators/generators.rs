use crate::test_case::{TestCase, labels};
use ciborium::Value;
use std::marker::PhantomData;
use std::sync::Arc;

/// A bundled schema + parse function for schema-based generation.
///
/// The lifetime `'a` ties the BasicGenerator to the generator that created it.
/// `T: 'a` is required because the parse closure returns `T`.
#[doc(hidden)]
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
    ///
    /// When the source generator has a schema (i.e. `as_basic()` returns `Some`),
    /// the schema is preserved and the function is composed into the parse step.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use hegel::generators::{self as gs, Generator};
    ///
    /// // Generate even integers by doubling
    /// let evens = gs::integers::<i32>().map(|n| n * 2);
    /// ```
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

    /// Generate a value, then use it to choose or configure another generator.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use hegel::generators::{self as gs, Generator};
    ///
    /// // Generate a length, then a vec of exactly that length
    /// let generator = gs::integers::<usize>()
    ///     .min_value(1)
    ///     .max_value(10)
    ///     .flat_map(|len| gs::vecs(gs::integers::<i32>())
    ///         .min_size(len)
    ///         .max_size(len));
    /// ```
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

    /// Only keep generated values that satisfy the predicate.
    ///
    /// Retries up to 3 times, then calls `assume(false)` to reject the test case.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use hegel::generators::{self as gs, Generator};
    ///
    /// // Generate integers, then filter out the even ones
    /// let odds = gs::integers::<i32>().filter(|n| n % 2 != 0);
    /// ```
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

    /// Return all possible values if this generator has a known finite value set.
    ///
    /// Used by [`Filtered`] as a fallback when random sampling fails: instead of
    /// calling `assume(false)`, enumerate valid elements and pick one.
    /// Mirrors Hypothesis's `SampledFromStrategy.do_filtered_draw` optimization.
    #[doc(hidden)]
    fn enumerate_values(&self) -> Option<Vec<T>> {
        None
    }

    /// Convert this generator into a type-erased boxed generator.
    ///
    /// This is needed when you have generators of different concrete types
    /// but the same output type and need to store them together, e.g. in a
    /// `Vec` or when passing to [`one_of()`](super::one_of).
    ///
    /// # Example
    ///
    /// ```no_run
    /// use hegel::generators::{self as gs, Generator};
    ///
    /// // Different generator types producing the same output type —
    /// // boxing lets them be stored in a vec and passed to one_of
    /// let generator = vec![
    ///     gs::integers::<i32>().min_value(0).max_value(10).boxed(),
    ///     gs::integers::<i32>().map(|n| n * 100).boxed(),
    ///     gs::sampled_from(vec![1, 2, 3]).boxed(),
    /// ];
    /// ```
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

    // nocov start
    fn as_basic(&self) -> Option<BasicGenerator<'_, T>> {
        (*self).as_basic()
        // nocov end
    }
}

/// Result of [`Generator::map`]. Preserves the schema when possible.
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

    fn enumerate_values(&self) -> Option<Vec<U>> {
        self.source
            .enumerate_values()
            .map(|vals| vals.into_iter().map(|v| (self.f)(v)).collect())
    }
}

/// Result of [`Generator::flat_map`].
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

/// Result of [`Generator::filter`].
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
        if let Some(valid) = self.enumerate_values() {
            if valid.is_empty() {
                panic!(
                    "Unsatisfiable filter: all values from the source generator \
                     are rejected by the filter predicate"
                );
            }
            let idx = super::integers::<usize>()
                .min_value(0)
                .max_value(valid.len() - 1)
                .do_draw(tc);
            return valid.into_iter().nth(idx).unwrap();
        }
        tc.assume(false);
        unreachable!()
    }

    fn enumerate_values(&self) -> Option<Vec<T>> {
        self.source
            .enumerate_values()
            .map(|vals| vals.into_iter().filter(|v| (self.predicate)(v)).collect())
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

    fn enumerate_values(&self) -> Option<Vec<T>> {
        self.inner.enumerate_values()
    }

    fn boxed<'b>(self) -> BoxedGenerator<'b, T>
    where
        Self: Sized + Send + Sync + 'b,
    {
        BoxedGenerator { inner: self.inner }
    }
}
