use proc_macro2::TokenStream;
use quote::quote;
use syn::{FnArg, ImplItemFn, PatType, Type};

use crate::common::{extract_ident_from_pat, rewrite_helper_draws_in_block};

fn is_test_case_type(ty: &Type) -> bool {
    match ty {
        Type::Reference(reference) => is_test_case_type(&reference.elem),
        Type::Path(path) => path
            .path
            .segments
            .last()
            .is_some_and(|segment| segment.ident == "TestCase"),
        _ => false,
    }
}

/// Expand `#[hegel::test_helper]` on a free function or method: rewrite
/// `let x = tc.draw(gen)` in the body to a named draw, the way
/// `#[hegel::test]` does for a test body. The test case parameter is found
/// by its type rather than by position, since helpers take other arguments.
pub fn expand_test_helper(attr: TokenStream, item: TokenStream) -> TokenStream {
    if !attr.is_empty() {
        return syn::Error::new_spanned(attr, "#[hegel::test_helper] takes no arguments")
            .to_compile_error();
    }

    let mut func: ImplItemFn = match syn::parse2(item) {
        Ok(f) => f,
        Err(e) => return e.to_compile_error(),
    };

    let mut test_case_params = func.sig.inputs.iter().filter_map(|arg| match arg {
        FnArg::Typed(pat_type) if is_test_case_type(&pat_type.ty) => Some(pat_type),
        _ => None,
    });
    let param: &PatType = match (test_case_params.next(), test_case_params.next()) {
        (Some(param), None) => param,
        (Some(_), Some(second)) => {
            return syn::Error::new_spanned(
                second,
                "#[hegel::test_helper] functions must take exactly one `TestCase` parameter.",
            )
            .to_compile_error();
        }
        (None, _) => {
            return syn::Error::new_spanned(
                &func.sig,
                "#[hegel::test_helper] functions must take a parameter of type \
                 `&hegel::TestCase`.",
            )
            .to_compile_error();
        }
    };

    let tc_ident = match extract_ident_from_pat(&param.pat) {
        Some(name) => name,
        None => {
            return syn::Error::new_spanned(
                &param.pat,
                "#[hegel::test_helper] requires the `TestCase` parameter to be a simple \
                 identifier.",
            )
            .to_compile_error();
        }
    };

    rewrite_helper_draws_in_block(&mut func.block, &tc_ident);

    quote! { #func }
}
