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

pub(crate) fn object_schema(
    properties: Vec<(proc_macro2::TokenStream, proc_macro2::TokenStream)>,
    required: Vec<proc_macro2::TokenStream>,
) -> proc_macro2::TokenStream {
    cbor_map(vec![
        (cbor_text("type"), cbor_text("object")),
        (cbor_text("properties"), cbor_map(properties)),
        (cbor_text("required"), cbor_array(required)),
    ])
}

// --- CBOR parsing helper ---

pub(crate) fn cbor_map_to_hashmap(
    var_name: &str,
    source: proc_macro2::TokenStream,
    error_msg: &str,
) -> proc_macro2::TokenStream {
    let var = format_ident!("{}", var_name);
    quote! {
        let mut #var: std::collections::HashMap<String, hegel::ciborium::Value> = match #source {
            hegel::ciborium::Value::Map(entries) => {
                entries.into_iter().filter_map(|(k, v)| {
                    if let hegel::ciborium::Value::Text(key) = k { Some((key, v)) } else { None }
                }).collect()
            }
            _ => panic!(concat!(#error_msg, ", got {:?}"), #source),
        };
    }
}

// --- Bounds generation ---

/// Generate DefaultGenerator + Send + Sync bounds for a set of types.
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
