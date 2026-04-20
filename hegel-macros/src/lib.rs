mod common;
mod composite;
mod enum_gen;
mod explicit_test_case;
mod hegel_main;
mod hegel_test;
mod rewrite_draws;
mod standalone_function;
mod stateful;
mod struct_gen;
mod utils;

use proc_macro::TokenStream;
use syn::{Data, DeriveInput, ItemFn, ItemImpl, parse_macro_input};

// documentation for hegel-macros lives in hegel's lib.rs so that we get proper intra-doc links.

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

#[proc_macro_attribute]
pub fn test(attr: TokenStream, item: TokenStream) -> TokenStream {
    hegel_test::expand_test(attr.into(), item.into()).into()
}

#[proc_macro_attribute]
pub fn main(attr: TokenStream, item: TokenStream) -> TokenStream {
    hegel_main::expand_main(attr.into(), item.into()).into()
}

#[proc_macro_attribute]
pub fn standalone_function(attr: TokenStream, item: TokenStream) -> TokenStream {
    standalone_function::expand_standalone_function(attr.into(), item.into()).into()
}

#[proc_macro_attribute]
pub fn composite(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as ItemFn);
    composite::expand_composite(input).into()
}

/// Define an explicit test case to run before the property-based test.
///
/// Note: We are not currently 100% happy with the name of this attribute
/// and expect that we might change it in future. The API should otherwise
/// remain compatible, but you might have to rename some call-sites.
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

/// Rewrite `tc.draw(gen)` calls inside a closure body to the named form used
/// by `#[hegel::test]`, so failing-test output prints real variable names.
///
/// Intended for test infrastructure that constructs `Hegel::new(...)` by hand
/// (e.g. when wrapping a test run with custom output capture) where the
/// ordinary `#[hegel::test]` attribute isn't an option.
///
/// ```ignore
/// let closure = hegel::rewrite_draws!(|tc: hegel::TestCase| {
///     let x: i32 = tc.draw(hegel::generators::integers());
///     assert!(x < 10);
/// });
/// ```
#[proc_macro]
pub fn rewrite_draws(input: TokenStream) -> TokenStream {
    rewrite_draws::expand_rewrite_draws(input.into()).into()
}
