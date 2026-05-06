use proc_macro2::TokenStream;
use quote::quote;
use syn::{FnArg, ItemFn};

use crate::common::{
    SettingsAttrArgs, build_explicit_blocks, extract_explicit_test_cases, extract_ident_from_pat,
    rewrite_draws_in_block,
};

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

    for attr in &func.attrs {
        if attr.path().is_ident("test") {
            return syn::Error::new_spanned(
                attr,
                "#[hegel::test] used on a function with #[test].\
                Remove the #[test] attribute; [hegel::test] automatically adds #[test].",
            )
            .to_compile_error();
        }
    }

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

    quote! {
        #[test]
        #func
    }
}
