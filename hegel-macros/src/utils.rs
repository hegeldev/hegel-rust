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

/// Returns true if `s` is a Rust keyword.
pub(crate) fn is_rust_keyword(s: &str) -> bool {
    matches!(
        s,
        // strict keywords
        "as" | "break" | "const" | "continue" | "crate" | "else"
        | "enum" | "extern" | "false" | "fn" | "for" | "if"
        | "impl" | "in" | "let" | "loop" | "match" | "mod"
        | "move" | "mut" | "pub" | "ref" | "return" | "self"
        | "Self" | "static" | "struct" | "super" | "trait" | "true"
        | "type" | "unsafe" | "use" | "where" | "while" | "async" | "await" | "dyn"
        // reserved keywords
        | "abstract" | "become" | "box" | "do" | "final" | "macro"
        | "override" | "priv" | "typeof" | "unsized" | "virtual"
        | "yield" | "try" | "gen"
    )
}

/// Returns true if `s` is a valid Rust method-name token.
///
/// Bare identifiers must not be Rust keywords. Raw identifiers (`r#name`) are
/// valid unless the bare part is one of the four reserved words that cannot
/// be raw (`self`, `Self`, `super`, `crate`).
pub(crate) fn is_valid_method_name(s: &str) -> bool {
    if let Some(bare) = s.strip_prefix("r#") {
        !matches!(bare, "self" | "Self" | "super" | "crate")
    } else {
        !is_rust_keyword(s)
    }
}

/// Construct a [`syn::Ident`] from a method-name string, handling the `r#`
/// prefix by routing to [`syn::Ident::new_raw`] when present.
pub(crate) fn make_method_ident(name: &str, span: proc_macro2::Span) -> syn::Ident {
    if let Some(bare) = name.strip_prefix("r#") {
        syn::Ident::new_raw(bare, span)
    } else {
        syn::Ident::new(name, span)
    }
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
    use super::{is_valid_method_name, pascal_to_snake};

    #[test]
    fn test_is_valid_method_name() {
        // bare keywords are invalid
        assert!(!is_valid_method_name("type"));
        assert!(!is_valid_method_name("super"));

        // bare non-keywords are valid
        assert!(is_valid_method_name("type_"));
        assert!(is_valid_method_name("read_write"));
        assert!(is_valid_method_name("Type")); // PascalCase, not a keyword

        // raw form of raw-able keywords is valid
        assert!(is_valid_method_name("r#type"));
        assert!(is_valid_method_name("r#async"));
        assert!(is_valid_method_name("r#gen"));

        // raw form of non-raw-able keywords is invalid
        assert!(!is_valid_method_name("r#super"));
        assert!(!is_valid_method_name("r#self"));
        assert!(!is_valid_method_name("r#crate"));
        assert!(!is_valid_method_name("r#Self"));

        // raw form of non-keywords is valid (decorative r#)
        assert!(is_valid_method_name("r#Type"));
        assert!(is_valid_method_name("r#read_write"));
        assert!(is_valid_method_name("r#type_"));
    }

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
