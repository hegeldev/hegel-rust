//! `#[derive(PrettyPrintable)]` — implements `hegel::PrettyPrintable` for a
//! struct or enum, printing it in Rust-expression syntax with the printer's
//! group machinery so large values wrap readably.
//!
//! Layout mirrors the standard-type implementations in `hegel::pretty`:
//! braced shapes print as `Name { field: value, … }` with block indentation
//! of 4 when broken, tuple shapes as `Name(value, …)` with continuation
//! indentation of 1, and enum variants are qualified as `Name::Variant` so
//! the output is a valid expression. Every generic type parameter gets a
//! `PrettyPrintable` bound, like `derive(Debug)` does with `Debug`.

use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{Data, DeriveInput, Fields, Index};

pub(crate) fn derive_pretty_printable(input: &DeriveInput) -> syn::Result<TokenStream> {
    let name = &input.ident;
    let mut generics = input.generics.clone();
    for param in generics.type_params_mut() {
        param
            .bounds
            .push(syn::parse_quote!(::hegel::PrettyPrintable));
    }
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    let body = match &input.data {
        Data::Struct(data) => {
            let accessors: Vec<TokenStream> = match &data.fields {
                Fields::Named(fields) => fields
                    .named
                    .iter()
                    .map(|f| {
                        let ident = &f.ident;
                        quote! { &self.#ident }
                    })
                    .collect(),
                Fields::Unnamed(fields) => (0..fields.unnamed.len())
                    .map(|i| {
                        let index = Index::from(i);
                        quote! { &self.#index }
                    })
                    .collect(),
                Fields::Unit => Vec::new(),
            };
            print_shape(&name.to_string(), &data.fields, &accessors)
        }
        Data::Enum(data) if data.variants.is_empty() => quote! { match *self {} },
        Data::Enum(data) => {
            let arms = data.variants.iter().map(|variant| {
                let variant_name = &variant.ident;
                let label = format!("{name}::{variant_name}");
                match &variant.fields {
                    Fields::Named(fields) => {
                        let idents: Vec<_> = fields
                            .named
                            .iter()
                            .map(|f| f.ident.clone().unwrap())
                            .collect();
                        let accessors: Vec<TokenStream> =
                            idents.iter().map(|i| quote! { #i }).collect();
                        let body = print_shape(&label, &variant.fields, &accessors);
                        quote! { #name::#variant_name { #(#idents),* } => { #body } }
                    }
                    Fields::Unnamed(fields) => {
                        let idents: Vec<_> = (0..fields.unnamed.len())
                            .map(|i| format_ident!("__field{i}"))
                            .collect();
                        let accessors: Vec<TokenStream> =
                            idents.iter().map(|i| quote! { #i }).collect();
                        let body = print_shape(&label, &variant.fields, &accessors);
                        quote! { #name::#variant_name(#(#idents),*) => { #body } }
                    }
                    Fields::Unit => {
                        let body = print_shape(&label, &variant.fields, &[]);
                        quote! { #name::#variant_name => { #body } }
                    }
                }
            });
            quote! {
                match self {
                    #(#arms)*
                }
            }
        }
        Data::Union(_) => {
            return Err(syn::Error::new_spanned(
                input,
                "PrettyPrintable cannot be derived for unions",
            ));
        }
    };

    Ok(quote! {
        #[automatically_derived]
        impl #impl_generics ::hegel::PrettyPrintable for #name #ty_generics #where_clause {
            fn pretty_print(&self, __printer: &mut ::hegel::PrettyPrinter) {
                #body
            }
        }
    })
}

/// Emit the printing statements for one struct or enum-variant shape.
/// `label` is the leading name (`Point`, `Shape::Circle`), and `accessors`
/// are expressions evaluating to `&FieldType` for each field in order. The
/// layout itself comes from [`crate::utils::print_shape`], which the
/// `DefaultGenerator` derive shares so a derived generator prints values in
/// exactly this format.
fn print_shape(label: &str, fields: &Fields, accessors: &[TokenStream]) -> TokenStream {
    let actions: Vec<TokenStream> = accessors
        .iter()
        .map(|accessor| quote! { ::hegel::PrettyPrintable::pretty_print(#accessor, __printer); })
        .collect();
    crate::utils::print_shape(label, fields, &actions)
}
