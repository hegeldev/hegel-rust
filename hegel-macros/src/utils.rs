use quote::{format_ident, quote};

// --- CBOR construction helpers ---

pub(crate) fn cbor_text(s: &str) -> proc_macro2::TokenStream {
    quote! { hegel::ciborium::Value::Text(#s.to_string()) }
}

pub(crate) fn cbor_map(
    entries: Vec<(proc_macro2::TokenStream, proc_macro2::TokenStream)>,
) -> proc_macro2::TokenStream {
    let pairs: Vec<_> = entries
        .into_iter()
        .map(|(k, v)| quote! { (#k, #v) })
        .collect();
    quote! { hegel::ciborium::Value::Map(vec![#(#pairs),*]) }
}

pub(crate) fn cbor_array(items: Vec<proc_macro2::TokenStream>) -> proc_macro2::TokenStream {
    quote! { hegel::ciborium::Value::Array(vec![#(#items),*]) }
}

// --- Schema construction helpers ---

pub(crate) fn tuple_schema(elements: Vec<proc_macro2::TokenStream>) -> proc_macro2::TokenStream {
    cbor_map(vec![
        (cbor_text("type"), cbor_text("tuple")),
        (cbor_text("elements"), cbor_array(elements)),
    ])
}

// --- CBOR parsing helpers ---

pub(crate) fn cbor_to_iter(
    var_name: &str,
    source: proc_macro2::TokenStream,
    error_msg: &str,
) -> proc_macro2::TokenStream {
    let var = format_ident!("{}", var_name);
    quote! {
        let mut #var = match #source {
            hegel::ciborium::Value::Array(arr) => arr.into_iter(),
            other => panic!(concat!(#error_msg, ", got {:?}"), other),
        };
    }
}

// --- Bounds generation ---

/// Generator DefaultGenerator + Send + Sync bounds for a set of types.
pub(crate) fn default_gen_bounds(
    types: &[&syn::Type],
    lifetime: proc_macro2::TokenStream,
) -> Vec<proc_macro2::TokenStream> {
    types
        .iter()
        .map(|ty| {
            quote! {
                #ty: hegel::generators::DefaultGenerator,
                <#ty as hegel::generators::DefaultGenerator>::Generator: Send + Sync + #lifetime
            }
        })
        .collect()
}
