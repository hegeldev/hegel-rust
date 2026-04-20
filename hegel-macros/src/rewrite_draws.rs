use proc_macro2::TokenStream;
use quote::quote;
use syn::{Expr, ExprClosure};

use crate::common::{extract_ident_from_pat, rewrite_draws_in_block};

/// Expand `hegel::rewrite_draws!(|tc| { ... })`.
///
/// Rewrites `let x = tc.draw(gen)` inside the closure body to
/// `let x = tc.__draw_named(gen, "x", repeatable)`, matching the rewriting
/// that `#[hegel::test]` performs on ordinary test function bodies. The
/// resulting closure is returned so the caller can pass it to something like
/// `Hegel::new(...)`.
pub fn expand_rewrite_draws(input: TokenStream) -> TokenStream {
    let mut closure: ExprClosure = match syn::parse2(input) {
        Ok(c) => c,
        Err(e) => return e.to_compile_error(),
    };

    if closure.inputs.len() != 1 {
        return syn::Error::new_spanned(
            &closure,
            "hegel::rewrite_draws! requires a closure taking exactly one parameter (the TestCase).",
        )
        .to_compile_error();
    }

    let tc_ident = match extract_ident_from_pat(&closure.inputs[0]) {
        Some(name) => name,
        None => {
            return syn::Error::new_spanned(
                &closure.inputs[0],
                "hegel::rewrite_draws! closure parameter must be a simple identifier.",
            )
            .to_compile_error();
        }
    };

    if let Expr::Block(block_expr) = &mut *closure.body {
        rewrite_draws_in_block(&mut block_expr.block, &tc_ident);
    }

    quote! { #closure }
}
