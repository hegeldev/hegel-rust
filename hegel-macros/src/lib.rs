mod composite;
mod enum_gen;
mod hegel_test;
mod struct_gen;
mod utils;

use proc_macro::TokenStream;
use syn::{Data, DeriveInput, ItemFn, parse_macro_input};

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

/// Mark a test function as a Hegel property-based test.
///
/// Wraps the function body in `Hegel::new(|tc: TestCase| { ... }).run()`. The function
/// must take exactly one parameter of type `hegel::TestCase`, and use `tc.draw()` to
/// generate values. The `#[test]` attribute is added automatically and must not be
/// present on the function.
///
/// Optionally accepts settings as `key = value` pairs:
///
/// ```ignore
/// #[hegel::test]
/// fn my_test(tc: hegel::TestCase) {
///     let x: i32 = tc.draw(generators::integers());
///     assert!(x + 0 == x);
/// }
///
/// #[hegel::test(test_cases = 500)]
/// fn my_configured_test(tc: hegel::TestCase) {
///     let x: i32 = tc.draw(generators::integers());
///     assert!(x + 0 == x);
/// }
/// ```
#[proc_macro_attribute]
pub fn test(attr: TokenStream, item: TokenStream) -> TokenStream {
    hegel_test::expand_test(attr.into(), item.into()).into()
}

#[proc_macro_attribute]
pub fn composite(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as ItemFn);
    composite::expand_composite(input).into()
}
