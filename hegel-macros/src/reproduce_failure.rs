use proc_macro2::TokenStream;
use syn::ItemFn;

use crate::explicit_test_case::has_hegel_test_attr;

/// This macro always produces a compile error when it actually runs.
///
/// In correct usage (`#[hegel::test]` above, `#[hegel::reproduce_failure(...)]`
/// below), `#[hegel::test]` processes first and consumes the
/// `reproduce_failure` attribute directly from `func.attrs`, so this macro
/// never executes.
///
/// If this macro DOES execute, it means either:
/// - Wrong order: `#[hegel::reproduce_failure(...)]` is above `#[hegel::test]`
/// - Bare function: no `#[hegel::test]` at all
pub fn expand_reproduce_failure(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let func: ItemFn = match syn::parse2(item) {
        Ok(f) => f,
        Err(e) => return e.to_compile_error(),
    };

    if has_hegel_test_attr(&func) {
        syn::Error::new_spanned(
            &func.sig,
            "#[hegel::reproduce_failure(...)] must appear below #[hegel::test], not above it.\n\
             Write:\n  \
               #[hegel::test]\n  \
               #[hegel::reproduce_failure(\"<blob>\")]\n  \
               fn my_test(tc: hegel::TestCase) { ... }",
        )
        .to_compile_error()
    } else {
        syn::Error::new_spanned(
            &func.sig,
            "#[hegel::reproduce_failure(...)] can only be used together with #[hegel::test].\n\
             Add #[hegel::test] above #[hegel::reproduce_failure(...)].",
        )
        .to_compile_error()
    }
}
