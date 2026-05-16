use proc_macro2::TokenStream;
use quote::quote;
use syn::{ItemFn, parse_quote};

pub fn expand_not_supported_on_native(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let mut func: ItemFn = match syn::parse2(item) {
        Ok(f) => f,
        Err(e) => return e.to_compile_error(),
    };
    // Prepend `#[cfg_attr(feature = "native", should_panic)]`. Under default
    // features the test runs as a normal `#[test]`; under `--features native`
    // the runner expects it to panic, so an unexpectedly-passing test fails
    // loudly. Composes with both `#[test]` and `#[hegel::test]` (which
    // preserves prior attributes on its generated `#[test] fn`).
    let attr: syn::Attribute = parse_quote! {
        #[cfg_attr(feature = "native", should_panic)]
    };
    func.attrs.insert(0, attr);
    quote!(#func)
}
