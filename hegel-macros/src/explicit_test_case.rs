use proc_macro2::TokenStream;
use syn::ItemFn;

/// Check if an attribute's path is `hegel::test`.
fn has_hegel_test_attr(func: &ItemFn) -> bool {
    func.attrs.iter().any(|attr| {
        let segments: Vec<_> = attr.path().segments.iter().collect();
        segments.len() == 2 && segments[0].ident == "hegel" && segments[1].ident == "test"
    })
}

/// This macro always produces a compile error when it actually runs.
///
/// In correct usage (`#[hegel::test]` above, `#[hegel::explicit_test_case]` below),
/// `#[hegel::test]` processes first and consumes the explicit_test_case attributes
/// directly from `func.attrs`, so this macro never executes.
///
/// If this macro DOES execute, it means either:
/// - Wrong order: `#[hegel::explicit_test_case]` is above `#[hegel::test]`
/// - Bare function: no `#[hegel::test]` at all
pub fn expand_explicit_test_case(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let func: ItemFn = match syn::parse2(item) {
        Ok(f) => f,
        Err(e) => return e.to_compile_error(),
    };

    if has_hegel_test_attr(&func) {
        // #[hegel::test] is below us, meaning we're in the wrong order.
        // (If it were above us, it would have consumed our attribute before we ran.)
        syn::Error::new_spanned(
            &func.sig,
            "#[hegel::explicit_test_case] must appear below #[hegel::test], not above it.\n\
             Write:\n  \
               #[hegel::test]\n  \
               #[hegel::explicit_test_case(...)]\n  \
               fn my_test(tc: hegel::TestCase) { ... }",
        )
        .to_compile_error()
    } else {
        // No #[hegel::test] at all.
        syn::Error::new_spanned(
            &func.sig,
            "#[hegel::explicit_test_case] can only be used together with #[hegel::test].\n\
             Add #[hegel::test] above #[hegel::explicit_test_case].",
        )
        .to_compile_error()
    }
}
