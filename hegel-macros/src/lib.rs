mod composite;
mod enum_gen;
mod explicit_test_case;
mod hegel_test;
mod stateful;
mod struct_gen;
mod utils;

use proc_macro::TokenStream;
use syn::{Data, DeriveInput, ItemFn, ItemImpl, parse_macro_input};

/// Derive a generator for a struct or enum.
///
/// This implements [`DefaultGenerator`](hegel::generators::DefaultGenerator) for the type,
/// allowing it to be used with [`default`](hegel::generators::default) via `default::<T>()`.
///
/// For structs, the generated generator has:
/// - `<field>(generator)` - builder method to customize each field's generator
///
/// For enums, the generated generator has:
/// - `default_<VariantName>()` - methods returning default variant generators
/// - `<VariantName>(generator)` - builder methods to customize variant generation
///
/// # Struct Example
///
/// ```ignore
/// use hegel::DefaultGenerator;
/// use hegel::generators::{self, DefaultGenerator as _, Generator as _};
///
/// #[derive(DefaultGenerator)]
/// struct Person {
///     name: String,
///     age: u32,
/// }
///
/// #[hegel::test]
/// fn generates_people(tc: hegel::TestCase) {
///     let generator = generators::default::<Person>()
///         .age(generators::integers::<u32>().min_value(0).max_value(120));
///     let person: Person = tc.draw(generator);
/// }
/// ```
///
/// # Enum Example
///
/// ```ignore
/// use hegel::DefaultGenerator;
/// use hegel::generators::{self, DefaultGenerator as _, Generator as _};
///
/// #[derive(DefaultGenerator)]
/// enum Status {
///     Pending,
///     Active { since: String },
///     Error { code: i32, message: String },
/// }
///
/// #[hegel::test]
/// fn generates_statuses(tc: hegel::TestCase) {
///     let generator = generators::default::<Status>()
///         .Active(
///             generators::default::<Status>()
///                 .default_Active()
///                 .since(generators::text().max_size(20))
///         );
///     let status: Status = tc.draw(generator);
/// }
/// ```
#[proc_macro_derive(DefaultGenerator)]
pub fn derive_generator(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    match &input.data {
        Data::Struct(data) => struct_gen::derive_struct_generator(&input, data),
        Data::Enum(data) => enum_gen::derive_enum_generator(&input, data),
        Data::Union(_) => syn::Error::new_spanned(&input, "Generator cannot be derived for unions")
            .to_compile_error()
            .into(),
    }
}

/// The main entrypoint into Hegel.
///
/// The function must take exactly one parameter of type `hegel::TestCase`. The test case can be
/// used to generate values via `tc.draw()`.
///
/// The `#[test]` attribute is added automatically and must not be present on the function.
///
/// ```ignore
/// #[hegel::test]
/// fn my_test(tc: TestCase) {
///     let x: i32 = tc.draw(integers());
///     assert!(x + 0 == x);
/// }
///
/// You can set settings with on `hegel::test`, corresponding to methods on [`Settings`](hegel::Settings):
///
/// #[hegel::test(test_cases = 500)]
/// fn test_runs_many_more_times(tc: TestCase) {
///     let x: i32 = tc.draw(integers());
///     assert!(x + 0 == x);
/// }
/// ```
#[proc_macro_attribute]
pub fn test(attr: TokenStream, item: TokenStream) -> TokenStream {
    hegel_test::expand_test(attr.into(), item.into()).into()
}

/// Define a composite generator from a function.
///
/// The first parameter must be `tc: TestCase` and is passed automatically
/// when the generator is drawn. Any additional parameters become parameters
/// of the returned factory function. The function must have an explicit
/// return type.
///
/// ```ignore
/// use hegel::generators;
///
/// #[hegel::composite]
/// fn sorted_vec(tc: hegel::TestCase, min_len: usize) -> Vec<i32> {
///     let mut v: Vec<i32> = tc.draw(generators::vecs(generators::integers()).min_size(min_len));
///     v.sort();
///     v
/// }
///
/// #[hegel::test]
/// fn test_sorted(tc: hegel::TestCase) {
///     let v = tc.draw(sorted_vec(3));
///     assert!(v.len() >= 3);
///     assert!(v.windows(2).all(|w| w[0] <= w[1]));
/// }
/// ```
#[proc_macro_attribute]
pub fn composite(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as ItemFn);
    composite::expand_composite(input).into()
}

/// Define an explicit test case to run before the property-based test.
///
/// Must be placed **below** `#[hegel::test]`. Multiple attributes are allowed.
///
/// ```ignore
/// #[hegel::test]
/// #[hegel::explicit_test_case(x = 42, y = "hello")]
/// fn my_test(tc: hegel::TestCase) {
///     let x: i32 = tc.draw(hegel::generators::integers());
///     let y: String = tc.draw(hegel::generators::text());
///     // ...
/// }
/// ```
///
/// Arguments correspond to the names they would be printed with in a failing
/// test case, so need suffixing if they're repeated. For example:
///
/// ```ignore
/// #[hegel::test]
/// #[hegel::explicit_test_case(x_1 = 1, x_2 = 2, x_3 = 4 )]
/// fn my_test(tc: hegel::TestCase) {
///     for _ in 0..3 {
///         let x: i32 = tc.draw(hegel::generators::integers());
///     }
/// }
/// ```
#[proc_macro_attribute]
pub fn explicit_test_case(attr: TokenStream, item: TokenStream) -> TokenStream {
    explicit_test_case::expand_explicit_test_case(attr.into(), item.into()).into()
}

#[proc_macro_attribute]
pub fn state_machine(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let block = parse_macro_input!(item as ItemImpl);
    stateful::expand_state_machine(block).into()
}
