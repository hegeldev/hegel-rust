use proc_macro2::TokenStream;
use quote::quote;
use syn::punctuated::Punctuated;
use syn::token::Comma;
use syn::{FnArg, ItemFn};

use crate::common::{
    SettingsAttrArgs, build_explicit_blocks, extract_explicit_test_cases, extract_ident_from_pat,
    rewrite_draws_in_block,
};

pub fn expand_standalone_function(attr: TokenStream, item: TokenStream) -> TokenStream {
    let settings_args: SettingsAttrArgs = if attr.is_empty() {
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

    if func.sig.inputs.is_empty() {
        return syn::Error::new_spanned(
            &func.sig,
            "#[hegel::standalone_function] functions must take at least one parameter of type hegel::TestCase (as the first parameter).",
        )
        .to_compile_error();
    }

    let tc_param = match &func.sig.inputs[0] {
        FnArg::Typed(pat_type) => pat_type,
        FnArg::Receiver(_) => {
            return syn::Error::new_spanned(
                &func.sig.inputs[0],
                "#[hegel::standalone_function] functions cannot have a self parameter.",
            )
            .to_compile_error();
        }
    };
    let tc_pat = (*tc_param.pat).clone();
    let tc_ty = (*tc_param.ty).clone();

    if let syn::ReturnType::Type(_, _) = &func.sig.output {
        return syn::Error::new_spanned(
            &func.sig.output,
            "#[hegel::standalone_function] functions must not have a return type; \
             the property test is expected to panic on failure and return `()` on success.",
        )
        .to_compile_error();
    }

    for attr in &func.attrs {
        if attr.path().is_ident("test") {
            return syn::Error::new_spanned(
                attr,
                "#[hegel::standalone_function] cannot be combined with #[test]. \
                 Use #[hegel::test] for test functions.",
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
        if let Some(test_case_name) = extract_ident_from_pat(&tc_pat) {
            rewrite_draws_in_block(&mut body, &test_case_name);
        }
        body
    };

    let fn_name = func.sig.ident.to_string();
    let settings_expr = settings_args.to_settings_expr();
    let explicit_blocks = build_explicit_blocks(&explicit_cases, &tc_pat, &body);

    let passthrough: Punctuated<FnArg, Comma> = func.sig.inputs.iter().skip(1).cloned().collect();

    let new_body: TokenStream = quote! {
        {
            let __hegel_settings = #settings_expr;
            if __hegel_settings.phases.contains(&hegel::Phase::Explicit) {
                #(#explicit_blocks)*
            }

            hegel::Hegel::new(move |#tc_pat: #tc_ty| #body)
            .settings(__hegel_settings)
            .__database_key(format!("{}::{}", module_path!(), #fn_name))
            .test_location(hegel::TestLocation {
                function: #fn_name.to_string(),
                file: file!().to_string(),
                class: module_path!().to_string(),
                begin_line: line!(),
            })
            .run();
        }
    };

    let new_block: syn::Block = syn::parse2(new_body).unwrap();

    let mut func = func;
    func.sig.inputs = passthrough;
    *func.block = new_block;

    quote! {
        #func
    }
}
