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
//!
//! A field of a type that cannot implement `PrettyPrintable` (a foreign
//! type, say) can opt out with `#[pretty(debug)]`: that field prints its
//! `Debug` representation through `hegel::pretty::print_debug_repr`, and
//! the generated impl requires the field's type to implement `Debug`.

use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{Data, DeriveInput, Fields, Index};

pub(crate) fn derive_pretty_printable(input: &DeriveInput) -> syn::Result<TokenStream> {
    let name = &input.ident;
    reject_pretty_attrs(&input.attrs, "types")?;
    let mut generics = input.generics.clone();
    for param in generics.type_params_mut() {
        param
            .bounds
            .push(syn::parse_quote!(::hegel::PrettyPrintable));
    }
    for ty in debug_field_types(&input.data)? {
        generics
            .make_where_clause()
            .predicates
            .push(syn::parse_quote!(#ty: ::core::fmt::Debug));
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
            print_shape(&name.to_string(), &data.fields, &accessors)?
        }
        Data::Enum(data) if data.variants.is_empty() => quote! { match *self {} },
        Data::Enum(data) => {
            let arms = data
                .variants
                .iter()
                .map(|variant| {
                    reject_pretty_attrs(&variant.attrs, "variants")?;
                    let variant_name = &variant.ident;
                    let label = format!("{name}::{variant_name}");
                    Ok(match &variant.fields {
                        Fields::Named(fields) => {
                            let idents: Vec<_> = fields
                                .named
                                .iter()
                                .map(|f| f.ident.clone().unwrap())
                                .collect();
                            let bindings: Vec<_> = (0..idents.len())
                                .map(|i| format_ident!("__field{i}"))
                                .collect();
                            let accessors: Vec<TokenStream> =
                                bindings.iter().map(|i| quote! { #i }).collect();
                            let body = print_shape(&label, &variant.fields, &accessors)?;
                            quote! {
                                #name::#variant_name { #(#idents: #bindings),* } => { #body }
                            }
                        }
                        Fields::Unnamed(fields) => {
                            let idents: Vec<_> = (0..fields.unnamed.len())
                                .map(|i| format_ident!("__field{i}"))
                                .collect();
                            let accessors: Vec<TokenStream> =
                                idents.iter().map(|i| quote! { #i }).collect();
                            let body = print_shape(&label, &variant.fields, &accessors)?;
                            quote! { #name::#variant_name(#(#idents),*) => { #body } }
                        }
                        Fields::Unit => {
                            let body = print_shape(&label, &variant.fields, &[])?;
                            quote! { #name::#variant_name => { #body } }
                        }
                    })
                })
                .collect::<syn::Result<Vec<_>>>()?;
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

/// Whether a field carries `#[pretty(debug)]`, rejecting any other
/// `#[pretty(..)]` form with a pointed error.
fn field_prints_as_debug(field: &syn::Field) -> syn::Result<bool> {
    let mut debug = false;
    for attr in &field.attrs {
        if !attr.path().is_ident("pretty") {
            continue;
        }
        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("debug") {
                debug = true;
                Ok(())
            } else {
                Err(meta.error(
                    "unsupported #[pretty(..)] option: the only supported form is #[pretty(debug)]",
                ))
            }
        })?;
    }
    Ok(debug)
}

/// `#[pretty(..)]` configures how a *field* prints; anywhere else it is a
/// mistake worth a real error instead of silence.
fn reject_pretty_attrs(attrs: &[syn::Attribute], position: &str) -> syn::Result<()> {
    for attr in attrs {
        if attr.path().is_ident("pretty") {
            return Err(syn::Error::new_spanned(
                attr,
                format!("#[pretty(..)] applies to fields, not {position}"),
            ));
        }
    }
    Ok(())
}

/// The types of every `#[pretty(debug)]` field, for `Debug` where-clause
/// predicates on the generated impl.
fn debug_field_types(data: &Data) -> syn::Result<Vec<&syn::Type>> {
    let fields: Vec<&syn::Field> = match data {
        Data::Struct(data) => data.fields.iter().collect(),
        Data::Enum(data) => data.variants.iter().flat_map(|v| v.fields.iter()).collect(),
        Data::Union(_) => Vec::new(),
    };
    let mut types = Vec::new();
    for field in fields {
        if field_prints_as_debug(field)? {
            types.push(&field.ty);
        }
    }
    Ok(types)
}

/// Emit the printing statements for one struct or enum-variant shape.
/// `label` is the leading name (`Point`, `Shape::Circle`), and `accessors`
/// are expressions evaluating to `&FieldType` for each field in order. The
/// layout itself comes from [`crate::utils::print_shape`], which the
/// `DefaultGenerator` derive shares so a derived generator prints values in
/// exactly this format. A `#[pretty(debug)]` field prints its `Debug`
/// representation instead of its `PrettyPrintable` one.
fn print_shape(
    label: &str,
    fields: &Fields,
    accessors: &[TokenStream],
) -> syn::Result<TokenStream> {
    let actions = fields
        .iter()
        .zip(accessors)
        .map(|(field, accessor)| {
            Ok(if field_prints_as_debug(field)? {
                quote! {
                    ::hegel::pretty::print_debug_repr(
                        &::std::format!("{:?}", #accessor),
                        __printer,
                    );
                }
            } else {
                quote! { ::hegel::PrettyPrintable::pretty_print(#accessor, __printer); }
            })
        })
        .collect::<syn::Result<Vec<TokenStream>>>()?;
    Ok(crate::utils::print_shape(label, fields, &actions))
}
