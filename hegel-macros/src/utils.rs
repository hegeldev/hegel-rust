use quote::{format_ident, quote};

/// Convert a PascalCase string to snake_case.
///
/// Inserts underscores at word boundaries detected by case transitions:
/// - Before an uppercase letter that follows a lowercase letter or digit:
///   `ReadWrite` → `read_write`, `Vec3D` → `vec3_d`
/// - Before an uppercase letter followed by a lowercase letter in an uppercase run:
///   `HTTPServer` → `http_server`
pub(crate) fn pascal_to_snake(s: &str) -> String {
    let mut result = String::new();
    let chars: Vec<char> = s.chars().collect();
    for (i, &c) in chars.iter().enumerate() {
        if c.is_ascii_uppercase() {
            if i > 0 {
                let prev = chars[i - 1];
                let prev_lower_or_digit = prev.is_ascii_lowercase() || prev.is_ascii_digit();
                let next_lower = i + 1 < chars.len() && chars[i + 1].is_ascii_lowercase();
                if prev_lower_or_digit || (prev.is_ascii_uppercase() && next_lower) {
                    result.push('_');
                }
            }
            result.extend(c.to_lowercase());
        } else {
            result.push(c);
        }
    }
    result
}

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

pub(crate) fn tuple_schema(elements: Vec<proc_macro2::TokenStream>) -> proc_macro2::TokenStream {
    cbor_map(vec![
        (cbor_text("type"), cbor_text("tuple")),
        (cbor_text("elements"), cbor_array(elements)),
    ])
}

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

#[cfg(test)]
mod tests {
    use super::pascal_to_snake;

    #[test]
    fn test_pascal_to_snake() {
        assert_eq!(pascal_to_snake("Circle"), "circle");
        assert_eq!(pascal_to_snake("Red"), "red");
        assert_eq!(pascal_to_snake("A"), "a");
        assert_eq!(pascal_to_snake("a"), "a");

        assert_eq!(pascal_to_snake("ReadWrite"), "read_write");
        assert_eq!(pascal_to_snake("WithValue"), "with_value");
        assert_eq!(pascal_to_snake("WithFields"), "with_fields");
        assert_eq!(pascal_to_snake("VecVariant"), "vec_variant");

        assert_eq!(pascal_to_snake("HTTPServer"), "http_server");
        assert_eq!(pascal_to_snake("IOError"), "io_error");
        assert_eq!(pascal_to_snake("XMLParser"), "xml_parser");
        assert_eq!(pascal_to_snake("XMLHttpRequest"), "xml_http_request");

        assert_eq!(pascal_to_snake("AB"), "ab");
        assert_eq!(pascal_to_snake("ABC"), "abc");
        assert_eq!(pascal_to_snake("IO"), "io");

        assert_eq!(pascal_to_snake("circle"), "circle");
        assert_eq!(pascal_to_snake("read_write"), "read_write");

        assert_eq!(pascal_to_snake("Vec3D"), "vec3_d");
        assert_eq!(pascal_to_snake("Abc123Def"), "abc123_def");
        assert_eq!(pascal_to_snake("FIELD_NAME11"), "field_name11");
        assert_eq!(pascal_to_snake("Abc123DEF456"), "abc123_def456");

        assert_eq!(pascal_to_snake("Ab"), pascal_to_snake("AB"));
        assert_eq!(pascal_to_snake("IOError"), pascal_to_snake("IoError"));
    }
}
