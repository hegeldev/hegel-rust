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
