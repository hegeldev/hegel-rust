use quote::quote;

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
        "as" | "break"
            | "const"
            | "continue"
            | "crate"
            | "else"
            | "enum"
            | "extern"
            | "false"
            | "fn"
            | "for"
            | "if"
            | "impl"
            | "in"
            | "let"
            | "loop"
            | "match"
            | "mod"
            | "move"
            | "mut"
            | "pub"
            | "ref"
            | "return"
            | "self"
            | "Self"
            | "static"
            | "struct"
            | "super"
            | "trait"
            | "true"
            | "type"
            | "unsafe"
            | "use"
            | "where"
            | "while"
            | "async"
            | "await"
            | "dyn"
            | "abstract"
            | "become"
            | "box"
            | "do"
            | "final"
            | "macro"
            | "override"
            | "priv"
            | "typeof"
            | "unsized"
            | "virtual"
            | "yield"
            | "try"
            | "gen"
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

/// The pieces of a derive input's generics the generated code needs, split
/// once so `struct_gen` and `enum_gen` thread them identically.
pub(crate) struct GenericsParts<'a> {
    /// The declared parameters with their bounds (`T: Clone, const N: usize`),
    /// for declaration positions. The generated generator prepends its own
    /// `'a` lifetime before these.
    pub gen_params: &'a syn::punctuated::Punctuated<syn::GenericParam, syn::token::Comma>,
    /// The bare parameter names (`T, N`), for use positions.
    pub param_uses: Vec<proc_macro2::TokenStream>,
    /// Just the type parameters' idents, for `T: 'a` / `T: 'static` bounds.
    pub type_param_idents: Vec<&'a syn::Ident>,
    /// The input's `where` clause predicates, appended to every generated
    /// `where` clause.
    pub user_predicates: Vec<&'a syn::WherePredicate>,
    /// The `<T, N>` type-argument tokens for naming the derived type itself
    /// (empty for a non-generic type).
    pub ty_generics: proc_macro2::TokenStream,
}

/// Split a derive input's generics into [`GenericsParts`]. Lifetime
/// parameters are rejected: a borrowing type cannot promise the `'static`
/// generator `DefaultGenerator` requires.
pub(crate) fn split_generics(generics: &syn::Generics) -> Result<GenericsParts<'_>, syn::Error> {
    if let Some(lt) = generics.lifetimes().next() {
        return Err(syn::Error::new_spanned(
            lt,
            "#[derive(DefaultGenerator)] does not support lifetime parameters: a borrowing \
             type cannot be produced by the 'static generator DefaultGenerator requires",
        ));
    }
    let param_uses = generics
        .params
        .iter()
        .map(|p| match p {
            syn::GenericParam::Type(t) => {
                let ident = &t.ident;
                quote! { #ident }
            }
            syn::GenericParam::Const(c) => {
                let ident = &c.ident;
                quote! { #ident }
            }
            syn::GenericParam::Lifetime(_) => unreachable!("lifetimes rejected above"),
        })
        .collect();
    let type_param_idents = generics.type_params().map(|t| &t.ident).collect();
    let user_predicates = generics
        .where_clause
        .as_ref()
        .map(|w| w.predicates.iter().collect())
        .unwrap_or_default();
    let (_, ty_generics, _) = generics.split_for_impl();
    Ok(GenericsParts {
        gen_params: &generics.params,
        param_uses,
        type_param_idents,
        user_predicates,
        ty_generics: quote! { #ty_generics },
    })
}

/// `DefaultGenerator` bounds for a set of field types, required wherever the
/// generated code instantiates the fields' default generators.
pub(crate) fn default_gen_bounds(types: &[&syn::Type]) -> Vec<proc_macro2::TokenStream> {
    types
        .iter()
        .map(|ty| {
            quote! {
                #ty: ::hegel::generators::DefaultGenerator
            }
        })
        .collect()
}

/// The generator type parameter standing in for one field's generator:
/// `__GName` for a field `name`, `__G0` for a tuple field `_0`. The `__G`
/// prefix keeps the parameter out of the way of the derived type's own
/// generics.
pub(crate) fn generator_param_ident(field_name: &str) -> syn::Ident {
    let mut name = String::from("__G");
    for segment in field_name.trim_start_matches("r#").split('_') {
        let mut chars = segment.chars();
        if let Some(first) = chars.next() {
            name.extend(first.to_uppercase());
            name.push_str(chars.as_str());
        }
    }
    quote::format_ident!("{}", name)
}

/// Emit the printing statements for one struct or enum-variant shape,
/// matching the layout `#[derive(PrettyPrintable)]` produces: braced shapes
/// as `Label { field: value, … }` with block indentation of 4 when broken,
/// tuple shapes as `Label(value, …)` with continuation indentation of 1.
///
/// `label` is the leading name (`Point`, `Shape::Circle`); `actions` are
/// statement blocks that print each field's value — for the `PrettyPrintable`
/// derive a `pretty_print` call, for the `DefaultGenerator` derive a
/// draw-and-print of the field's generator — in declaration order.
pub(crate) fn print_shape(
    label: &str,
    fields: &syn::Fields,
    actions: &[proc_macro2::TokenStream],
) -> proc_macro2::TokenStream {
    match fields {
        syn::Fields::Unit => quote! { __printer.text(#label); },
        syn::Fields::Named(named) if named.named.is_empty() => {
            let text = format!("{label} {{}}");
            quote! { __printer.text(#text); }
        }
        syn::Fields::Named(named) => {
            let open = format!("{label} {{");
            let steps =
                named
                    .named
                    .iter()
                    .zip(actions)
                    .enumerate()
                    .map(|(index, (field, action))| {
                        let prefix = format!("{}: ", field.ident.as_ref().unwrap());
                        let separator = if index > 0 {
                            quote! {
                                __printer.text(",");
                                __printer.breakable(" ");
                            }
                        } else {
                            quote! {}
                        };
                        quote! {
                            #separator
                            __printer.text(#prefix);
                            #action
                        }
                    });
            quote! {
                __printer.begin_group(4, #open);
                __printer.breakable(" ");
                #(#steps)*
                __printer.end_group(" }");
            }
        }
        syn::Fields::Unnamed(_) => {
            let open = format!("{label}(");
            let steps = actions.iter().enumerate().map(|(index, action)| {
                let separator = if index > 0 {
                    quote! {
                        __printer.text(",");
                        __printer.breakable(" ");
                    }
                } else {
                    quote! {}
                };
                quote! {
                    #separator
                    #action
                }
            });
            quote! {
                __printer.begin_group(1, #open);
                #(#steps)*
                __printer.end_group(")");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{is_valid_method_name, pascal_to_snake};

    #[test]
    fn test_is_valid_method_name() {
        assert!(!is_valid_method_name("type"));
        assert!(!is_valid_method_name("super"));

        assert!(is_valid_method_name("type_"));
        assert!(is_valid_method_name("read_write"));
        assert!(is_valid_method_name("Type"));

        assert!(is_valid_method_name("r#type"));
        assert!(is_valid_method_name("r#async"));
        assert!(is_valid_method_name("r#gen"));

        assert!(!is_valid_method_name("r#super"));
        assert!(!is_valid_method_name("r#self"));
        assert!(!is_valid_method_name("r#crate"));
        assert!(!is_valid_method_name("r#Self"));

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
