use proc_macro2::TokenStream;
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::{Expr, Ident, ItemFn, Token};

/// A single setting in a `#[hegel::test(...)]` expression.
struct Setting {
    key: Ident,
    value: Expr,
}

/// Parsed result of `#[hegel::test(key = value, ...)]`.
struct SettingsArgs {
    settings: Vec<Setting>,
}

impl Parse for SettingsArgs {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut settings = Vec::new();
        while !input.is_empty() {
            let key: Ident = input.parse()?;
            let _eq: Token![=] = input.parse()?;
            let value: Expr = input.parse()?;
            settings.push(Setting { key, value });
            if !input.is_empty() {
                let _comma: Token![,] = input.parse()?;
            }
        }
        Ok(SettingsArgs { settings })
    }
}

pub fn expand_test(
    attr: proc_macro2::TokenStream,
    item: proc_macro2::TokenStream,
) -> TokenStream {
    let settings_args: SettingsArgs = if attr.is_empty() {
        SettingsArgs {
            settings: Vec::new(),
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

    if !func.sig.inputs.is_empty() {
        return syn::Error::new_spanned(
            &func.sig.inputs,
            "#[hegel::test] functions must not have parameters.",
        )
        .to_compile_error();
    }

    let body = &func.block;

    let settings_chain: Vec<TokenStream> = settings_args
        .settings
        .iter()
        .map(|s| {
            let key = &s.key;
            let value = &s.value;
            quote! { .#key(#value) }
        })
        .collect();

    let new_body: TokenStream = quote! {
        {
            hegel::Hegel::new(|| #body)
            #(#settings_chain)*
            .run();
        }
    };

    let new_block: syn::Block = syn::parse2(new_body).expect("failed to parse generated body");

    let mut func = func;
    func.block = Box::new(new_block);

    // Remove #[hegel::test] attribute from output (it's already consumed by the proc macro)
    func.attrs.retain(|attr| {
        let path = attr.path();
        !path.is_ident("test")
            && !path
                .segments
                .last()
                .map(|s| s.ident == "test")
                .unwrap_or(false)
    });

    quote! { #func }
}
