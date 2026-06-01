use super::*;
use quote::quote;

fn rewrite(input: proc_macro2::TokenStream) -> String {
    let mut block: syn::Block = syn::parse2(quote! { { #input } }).unwrap();
    rewrite_draws_in_block(&mut block, "tc");
    quote! { #block }.to_string()
}

#[test]
fn test_target_call_is_rewritten_with_source_label() {
    let out = rewrite(quote! {
        tc.target(n as f64);
    });
    assert!(
        out.contains("target_labelled"),
        "expected method renamed to target_labelled, got: {out}"
    );
    assert!(
        out.contains("\"n as f64\""),
        "expected label to be the source of the score expression, got: {out}"
    );
}

#[test]
fn test_target_rewrite_uses_full_expression_source() {
    let out = rewrite(quote! {
        tc.target((m + n) as f64 / 2.0);
    });
    assert!(
        out.contains("\"(m + n) as f64 / 2.0\""),
        "expected literal expression in label, got: {out}"
    );
}

#[test]
fn test_target_labelled_call_is_left_alone() {
    let out = rewrite(quote! {
        tc.target_labelled(score, "explicit");
    });
    assert!(
        !out.contains("target_labelled (score , \"explicit\" , \""),
        "target_labelled must not be rewritten further, got: {out}"
    );
    assert!(
        out.contains("target_labelled (score , \"explicit\")"),
        "target_labelled call should be unchanged, got: {out}"
    );
}

#[test]
fn test_target_with_two_args_is_left_alone() {
    let out = rewrite(quote! {
        tc.target(score, ignored);
    });
    assert!(
        out.contains("tc . target (score , ignored)"),
        "two-arg target call must not be rewritten, got: {out}"
    );
}

#[test]
fn test_target_on_other_receiver_is_left_alone() {
    let out = rewrite(quote! {
        other.target(123.0);
    });
    assert!(
        out.contains("other . target (123.0)"),
        "non-tc receiver must not be rewritten, got: {out}"
    );
    assert!(
        !out.contains("target_labelled"),
        "non-tc receiver must not pick up target_labelled, got: {out}"
    );
}

#[test]
fn test_target_in_nested_block_is_rewritten() {
    let out = rewrite(quote! {
        for _ in 0..3 {
            tc.target(score);
        }
    });
    assert!(
        out.contains("target_labelled (score , \"score\")"),
        "target inside nested block should be rewritten, got: {out}"
    );
}

#[test]
fn test_target_inside_nested_fn_is_left_alone() {
    let out = rewrite(quote! {
        fn helper(tc: &TestCase) {
            tc.target(n as f64);
        }
    });
    assert!(
        !out.contains("target_labelled"),
        "rewriting must not descend into nested fn items, got: {out}"
    );
}

fn rendered(blob: Option<syn::Expr>) -> String {
    quote! { #blob }.to_string()
}

#[test]
fn extract_reproduce_failure_pulls_out_a_string_literal_and_removes_the_attr() {
    let mut attrs: Vec<syn::Attribute> =
        vec![syn::parse_quote!(#[hegel::reproduce_failure("AAEC")])];
    let blob = extract_reproduce_failure(&mut attrs).unwrap();
    assert_eq!(rendered(blob), r#""AAEC""#);
    assert!(attrs.is_empty(), "the attribute should be consumed");
}

#[test]
fn extract_reproduce_failure_accepts_a_variable_or_const() {
    // Not just literals — any expression (e.g. a const/variable) is allowed.
    let mut attrs: Vec<syn::Attribute> =
        vec![syn::parse_quote!(#[hegel::reproduce_failure(MY_BLOB)])];
    let blob = extract_reproduce_failure(&mut attrs).unwrap();
    assert_eq!(rendered(blob), "MY_BLOB");
}

#[test]
fn extract_reproduce_failure_returns_none_when_absent() {
    let mut attrs: Vec<syn::Attribute> = vec![syn::parse_quote!(#[inline])];
    let blob = extract_reproduce_failure(&mut attrs).unwrap();
    assert!(blob.is_none());
    assert_eq!(attrs.len(), 1, "unrelated attributes are left in place");
}

#[test]
fn extract_reproduce_failure_rejects_more_than_one() {
    let mut attrs: Vec<syn::Attribute> = vec![
        syn::parse_quote!(#[hegel::reproduce_failure("a")]),
        syn::parse_quote!(#[hegel::reproduce_failure("b")]),
    ];
    assert!(extract_reproduce_failure(&mut attrs).is_err());
}

#[test]
fn extract_reproduce_failure_rejects_empty_args() {
    // No expression at all isn't a valid blob argument.
    let mut attrs: Vec<syn::Attribute> = vec![syn::parse_quote!(#[hegel::reproduce_failure()])];
    assert!(extract_reproduce_failure(&mut attrs).is_err());
}
