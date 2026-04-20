use proc_macro2::TokenStream;
use quote::quote;
use syn::{FnArg, ItemFn};

use crate::common::{
    SettingsAttrArgs, build_explicit_blocks, extract_explicit_test_cases, extract_ident_from_pat,
    rewrite_draws_in_block,
};

pub fn expand_main(attr: TokenStream, item: TokenStream) -> TokenStream {
    let main_args: SettingsAttrArgs = if attr.is_empty() {
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
            "#[hegel::main] functions must take exactly one parameter of type hegel::TestCase.",
        )
        .to_compile_error();
    }

    let param = &func.sig.inputs[0];
    let param_typed = match param {
        FnArg::Typed(pat_type) => pat_type,
        FnArg::Receiver(_) => {
            return syn::Error::new_spanned(
                param,
                "#[hegel::main] functions cannot have a self parameter.",
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
                "#[hegel::main] used on a function with #[test]. \
                 Remove the #[test] attribute.",
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

    let fn_name = func.sig.ident.to_string();
    let default_settings_expr = main_args.to_settings_expr();
    let explicit_blocks = build_explicit_blocks(&explicit_cases, param_pat, &body);

    let new_body: TokenStream = quote! {
        {
            let __hegel_default_settings: hegel::Settings = #default_settings_expr;
            let __hegel_settings: hegel::Settings = match hegel::__apply_cli_args(
                __hegel_default_settings,
                ::std::env::args(),
            ) {
                hegel::CliOutcome::Success(s) => s,
                hegel::CliOutcome::Help(msg) => {
                    println!("{}", msg);
                    ::std::process::exit(0);
                }
                hegel::CliOutcome::ParseError(msg) => {
                    eprintln!("{}", msg);
                    ::std::process::exit(2);
                }
            };

            #(#explicit_blocks)*

            hegel::Hegel::new(|#param_pat: #param_ty| #body)
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
    func.sig.inputs.clear();
    *func.block = new_block;

    quote! {
        #func
    }
}
