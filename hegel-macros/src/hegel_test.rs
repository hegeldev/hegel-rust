use proc_macro2::TokenStream;
use quote::quote;
use syn::{Attribute, FnArg, ItemFn};

use crate::common::{
    SettingsAttrArgs, build_explicit_blocks, extract_explicit_test_cases, extract_ident_from_pat,
    rewrite_draws_in_block,
};

// Vendored from tokio 1fc450aefba4b05cdff9b7825ca5e39cccb3780e (thanks!)
//
// Check whether given attribute is a test attribute of forms:
// * `#[test]`
// * `#[core::prelude::*::test]` or `#[::core::prelude::*::test]`
// * `#[std::prelude::*::test]` or `#[::std::prelude::*::test]`
fn is_test_attribute(attr: &Attribute) -> bool {
    let path = match &attr.meta {
        syn::Meta::Path(path) => path,
        _ => return false,
    };
    let candidates = [
        ["core", "prelude", "*", "test"],
        ["std", "prelude", "*", "test"],
    ];
    if path.leading_colon.is_none()
        && path.segments.len() == 1
        && path.segments[0].arguments.is_none()
        && path.segments[0].ident == "test"
    {
        return true;
    } else if path.segments.len() != candidates[0].len() {
        return false;
    }
    candidates.into_iter().any(|segments| {
        path.segments.iter().zip(segments).all(|(segment, path)| {
            segment.arguments.is_none() && (path == "*" || segment.ident == path)
        })
    })
}

pub fn expand_test(attr: TokenStream, item: TokenStream) -> TokenStream {
    let test_args: SettingsAttrArgs = if attr.is_empty() {
        SettingsAttrArgs {
            settings: None,
            settings_args: Vec::new(),
        }
    } else {
        match syn::parse2(attr) {
            Ok(args) => args,
            Err(e) => return e.to_compile_error(),
        }
    };

    let mut func: ItemFn = match syn::parse2(item) {
        Ok(f) => f,
        Err(e) => return e.to_compile_error(),
    };

    if func.sig.inputs.len() != 1 {
        return syn::Error::new_spanned(
            &func.sig,
            "#[hegel::test] functions must take exactly one parameter of type hegel::TestCase.",
        )
        .to_compile_error();
    }

    let param = &func.sig.inputs[0];
    let param_typed = match param {
        FnArg::Typed(pat_type) => pat_type,
        FnArg::Receiver(_) => {
            return syn::Error::new_spanned(
                param,
                "#[hegel::test] functions cannot have a self parameter.",
            )
            .to_compile_error();
        }
    };
    let param_pat = &*param_typed.pat;
    let param_ty = &*param_typed.ty;

    // If #[tokio::test] (or similar) did its job, we shouldn't be seeing an async function.
    if let Some(asy) = func.sig.asyncness {
        return syn::Error::new_spanned(
            asy,
            "#[hegel::test] does not support bare interactions with async functions.\
             Put #[hegel::test] below an async test macro like #[tokio::test] instead.",
        )
        .to_compile_error();
    }

    let is_existing_test = func.attrs.iter().any(is_test_attribute);

    let explicit_cases = match extract_explicit_test_cases(&mut func.attrs) {
        Ok(cases) => cases,
        Err(err) => return err,
    };

    let body = {
        let mut body = (*func.block).clone();
        if let Some(test_case_name) = extract_ident_from_pat(param_pat) {
            rewrite_draws_in_block(&mut body, &test_case_name);
        }
        body
    };

    let test_name = func.sig.ident.to_string();
    let settings_expr = test_args.to_settings_expr();
    let explicit_blocks = build_explicit_blocks(&explicit_cases, param_pat, &body);

    let new_body: TokenStream = quote! {
        {
            let __hegel_settings = #settings_expr;
            if __hegel_settings.has_phase(hegel::Phase::Explicit) {
                #(#explicit_blocks)*
            }

            hegel::Hegel::new(|#param_pat: #param_ty| #body)
            .settings(__hegel_settings)
            .__database_key(format!("{}::{}", module_path!(), #test_name))
            .test_location(hegel::TestLocation {
                function: #test_name.to_string(),
                file: file!().replace('\\', "/"),
                class: module_path!().to_string(),
                begin_line: line!(),
            })
            .run();
        }
    };

    let new_block: syn::Block = syn::parse2(new_body).unwrap();

    let mut func = func;
    func.sig.inputs.clear();
    *func.block = new_block;

    if is_existing_test {
        quote! {
            #func
        }
    } else {
        quote! {
            #[test]
            #func
        }
    }
}
