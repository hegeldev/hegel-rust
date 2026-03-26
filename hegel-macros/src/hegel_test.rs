use proc_macro2::TokenStream;
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::{Expr, FnArg, Ident, ItemFn, Token};

/// A single named argument in a `#[hegel::test(...)]` expression.
struct SettingArg {
    key: Ident,
    value: Expr,
}

/// Parsed result of `#[hegel::test(...)]`.
///
/// Acceptable formats:
/// - `#[hegel::test]`
/// - `#[hegel::test(settings_expr)]`
/// - `#[hegel::test(settings_expr, seed = 42)]`
/// - `#[hegel::test(seed = 42)]`
struct TestArgs {
    settings: Option<Expr>,
    settings_args: Vec<SettingArg>,
}

impl Parse for TestArgs {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut settings = None;
        let mut settings_args = Vec::new();

        // check if the first arg is a settings expression or a named settingArg
        let is_named_arg = input.peek(Ident) && input.peek2(Token![=]);
        if !is_named_arg {
            settings = Some(input.parse::<Expr>()?);
            if !input.is_empty() {
                let _comma: Token![,] = input.parse()?;
            }
        }

        while !input.is_empty() {
            let key: Ident = input.parse()?;
            let _eq: Token![=] = input.parse()?;
            let value: Expr = input.parse()?;
            settings_args.push(SettingArg { key, value });
            if !input.is_empty() {
                let _comma: Token![,] = input.parse()?;
            }
        }

        Ok(TestArgs {
            settings,
            settings_args,
        })
    }
}

pub fn expand_test(attr: proc_macro2::TokenStream, item: proc_macro2::TokenStream) -> TokenStream {
    let test_args: TestArgs = if attr.is_empty() {
        TestArgs {
            settings: None,
            settings_args: Vec::new(),
        }
    } else {
        match syn::parse2(attr) {
            Ok(args) => args,
            Err(e) => return e.to_compile_error(),
        }
    };

    let func: ItemFn = match syn::parse2(item) {
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
    let param_pat = &param_typed.pat;
    let param_ty = &param_typed.ty;

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

    let body = &func.block;
    let test_name = func.sig.ident.to_string();

    let settings_args_chain: Vec<TokenStream> = test_args
        .settings_args
        .iter()
        .map(|arg| {
            let key = &arg.key;
            let value = &arg.value;
            quote! { .#key(#value) }
        })
        .collect();

    let settings_expr = match &test_args.settings {
        Some(expr) => quote! { #expr #(#settings_args_chain)* },
        None if settings_args_chain.is_empty() => quote! { hegel::Settings::new() },
        None => quote! { hegel::Settings::new() #(#settings_args_chain)* },
    };

    let new_body: TokenStream = quote! {
        {
            hegel::Hegel::new(|#param_pat: #param_ty| #body)
            .settings(#settings_expr)
            .__database_key(format!("{}::{}", module_path!(), #test_name))
            .test_location(hegel::TestLocation {
                function: #test_name.to_string(),
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
        #[test]
        #func
    }
}
