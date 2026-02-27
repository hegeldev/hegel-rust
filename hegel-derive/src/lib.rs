mod enum_gen;
mod hegel_test;
mod struct_gen;
mod utils;

use proc_macro::TokenStream;
use syn::{parse_macro_input, Data, DeriveInput};

/// Derive a generator for a struct or enum.
///
/// This implements [`DefaultGenerator`](hegel::generators::DefaultGenerator) for the type,
/// allowing it to be used with [`from_type`](hegel::generators::from_type) via `from_type::<T>()`.
///
/// For structs, the generated generator has:
/// - `with_<field>(gen)` - builder method to customize each field's generator
///
/// For enums, the generated generator has:
/// - `default_<VariantName>()` - methods returning default variant generators
/// - `with_<VariantName>(gen)` - builder methods to customize variant generation
///
/// # Struct Example
///
/// ```ignore
/// use hegel::Generate;
/// use hegel::generators::{self, Generate as _};
///
/// #[derive(Generate)]
/// struct Person {
///     name: String,
///     age: u32,
/// }
///
/// let gen = generators::from_type::<Person>()
///     .with_age(generators::integers::<u32>().with_min(0).with_max(120));
///
/// let person: Person = hegel::draw(&gen);
/// ```
///
/// # Enum Example
///
/// ```ignore
/// use hegel::Generate;
/// use hegel::generators::{self, Generate as _};
///
/// #[derive(Generate)]
/// enum Status {
///     Pending,
///     Active { since: String },
///     Error { code: i32, message: String },
/// }
///
/// let gen = generators::from_type::<Status>()
///     .with_Active(
///         generators::from_type::<Status>()
///             .default_Active()
///             .with_since(generators::text().with_max_size(20))
///     );
///
/// let status: Status = hegel::draw(&gen);
/// ```
#[proc_macro_derive(Generate)]
pub fn derive_generate(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    match &input.data {
        Data::Struct(data) => struct_gen::derive_struct_generate(&input, data),
        Data::Enum(data) => enum_gen::derive_enum_generate(&input, data),
        Data::Union(_) => syn::Error::new_spanned(&input, "Generate cannot be derived for unions")
            .to_compile_error()
            .into(),
    }
}

/// Mark a test function as a Hegel property-based test.
///
/// Wraps the function body in `Hegel::new(|| { ... }).run()`. Use `hegel::draw()`
/// inside the body to generate values.
///
/// Optionally accepts settings as `key = value` pairs:
///
/// ```ignore
/// #[hegel::test]
/// #[test]
/// fn my_test() {
///     let x: i32 = hegel::draw(&generators::integers());
///     assert!(x + 0 == x);
/// }
///
/// #[hegel::test(test_cases = 500)]
/// #[test]
/// fn my_configured_test() {
///     let x: i32 = hegel::draw(&generators::integers());
///     assert!(x + 0 == x);
/// }
/// ```
#[proc_macro_attribute]
pub fn test(attr: TokenStream, item: TokenStream) -> TokenStream {
    hegel_test::expand_test(attr.into(), item.into()).into()
}
