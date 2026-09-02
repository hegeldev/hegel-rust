use super::*;
use quote::quote;

fn expand(input: proc_macro2::TokenStream) -> String {
    let f: syn::ItemFn = syn::parse2(input).unwrap();
    let output = expand_composite(f);
    syn::parse2::<syn::File>(output.clone()).unwrap_or_else(|e| {
        panic!("expansion is not valid Rust: {e}\n{output}");
    });
    output.to_string()
}

fn expand_error(input: proc_macro2::TokenStream) -> String {
    let f: syn::ItemFn = syn::parse2(input).unwrap();
    let output = expand_composite(f).to_string();
    assert!(
        output.contains("compile_error !"),
        "expected a compile error, got: {output}"
    );
    output
}

fn assert_contains_tokens(output: &str, expected: proc_macro2::TokenStream) {
    let expected = expected.to_string();
    assert!(
        output.contains(&expected),
        "expected expansion to contain `{expected}`, got: {output}"
    );
}

#[test]
fn test_generates_named_struct_and_constructor() {
    let out = expand(quote! {
        fn tree(tc: &TestCase) -> BinTree {
            tc.draw(gs::just(BinTree::Leaf()))
        }
    });
    assert_contains_tokens(&out, quote! { struct TreeCompositeGenerator });
    assert_contains_tokens(&out, quote! { fn tree() -> TreeCompositeGenerator });
    assert_contains_tokens(
        &out,
        quote! { impl ::hegel::generators::Generator<BinTree> for TreeCompositeGenerator },
    );
}

#[test]
fn test_do_draw_wraps_body_call_in_span() {
    let out = expand(quote! {
        fn tree(tc: &TestCase) -> BinTree {
            tc.draw(gs::just(BinTree::Leaf()))
        }
    });
    assert_contains_tokens(&out, quote! { tc.start_span(__HEGEL_COMPOSITE_LABEL) });
    assert_contains_tokens(&out, quote! { tc.stop_span(false) });
    let start = out.find("start_span").unwrap();
    let call = out.find("Self :: __hegel_body").unwrap();
    let stop = out.find("stop_span").unwrap();
    assert!(
        start < call && call < stop,
        "body call must sit between start_span and stop_span: {out}"
    );
}

#[test]
fn test_body_keeps_test_case_assertion() {
    let out = expand(quote! {
        fn tree(tc: &TestCase) -> BinTree {
            tc.draw(gs::just(BinTree::Leaf()))
        }
    });
    assert_contains_tokens(
        &out,
        quote! { ::hegel::__assert_is_test_case::<TestCase>() },
    );
}

#[test]
fn test_passthrough_args_become_fields_cloned_per_draw() {
    let out = expand(quote! {
        fn bounded(tc: &TestCase, max_size: usize, name: String) -> i32 {
            tc.draw(gs::integers::<i32>())
        }
    });
    assert_contains_tokens(&out, quote! { max_size: usize, name: String });
    assert_contains_tokens(
        &out,
        quote! { fn bounded(max_size: usize, name: String) -> BoundedCompositeGenerator },
    );
    assert_contains_tokens(
        &out,
        quote! {
            Self::__hegel_body(
                tc,
                ::core::clone::Clone::clone(&self.max_size),
                ::core::clone::Clone::clone(&self.name)
            )
        },
    );
    assert_contains_tokens(
        &out,
        quote! { where usize: ::core::clone::Clone, String: ::core::clone::Clone },
    );
}

#[test]
fn test_struct_is_clone_when_args_are() {
    let out = expand(quote! {
        fn bounded(tc: &TestCase, max_size: usize) -> i32 {
            tc.draw(gs::integers::<i32>())
        }
    });
    assert_contains_tokens(
        &out,
        quote! { impl ::core::clone::Clone for BoundedCompositeGenerator },
    );
}

#[test]
fn test_snake_case_name_converts_to_pascal_case() {
    let out = expand(quote! {
        fn http_server_names2(tc: &TestCase) -> String {
            tc.draw(gs::text())
        }
    });
    assert_contains_tokens(&out, quote! { struct HttpServerNames2CompositeGenerator });
}

#[test]
fn test_visibility_is_inherited() {
    let out = expand(quote! {
        pub fn tree(tc: &TestCase) -> BinTree {
            tc.draw(gs::just(BinTree::Leaf()))
        }
    });
    assert_contains_tokens(&out, quote! { pub struct TreeCompositeGenerator });
    assert_contains_tokens(&out, quote! { pub fn tree() -> TreeCompositeGenerator });
}

#[test]
fn test_attributes_stay_on_constructor() {
    let out = expand(quote! {
        #[doc = "Generates trees."]
        fn tree(tc: &TestCase) -> BinTree {
            tc.draw(gs::just(BinTree::Leaf()))
        }
    });
    assert_contains_tokens(
        &out,
        quote! { #[doc = "Generates trees."] fn tree() -> TreeCompositeGenerator },
    );
}

#[test]
fn test_struct_gets_generated_doc_link() {
    let out = expand(quote! {
        fn tree(tc: &TestCase) -> BinTree {
            tc.draw(gs::just(BinTree::Leaf()))
        }
    });
    assert!(
        out.contains("Generator returned by [`tree()`]."),
        "struct should carry a doc link to the source fn: {out}"
    );
}

#[test]
fn test_generic_args_are_supported() {
    let out = expand(quote! {
        fn wrapped<G: Generator<i64>>(tc: &TestCase, inner: G) -> i64 {
            tc.draw(&inner)
        }
    });
    assert_contains_tokens(
        &out,
        quote! { struct WrappedCompositeGenerator<G: Generator<i64> > },
    );
    assert_contains_tokens(&out, quote! { inner: G });
    assert_contains_tokens(&out, quote! { ::core::marker::PhantomData });
}

#[test]
fn test_generic_used_only_in_return_type_is_carried_by_marker() {
    let out = expand(quote! {
        fn defaulted<T: Default>(tc: &TestCase) -> T {
            T::default()
        }
    });
    assert_contains_tokens(
        &out,
        quote! { struct DefaultedCompositeGenerator<T: Default> },
    );
    assert_contains_tokens(&out, quote! { fn() -> (T,) });
}

#[test]
fn test_where_clause_is_preserved() {
    let out = expand(quote! {
        fn wrapped<G>(tc: &TestCase, inner: G) -> i64
        where
            G: Generator<i64>,
        {
            tc.draw(&inner)
        }
    });
    assert_contains_tokens(&out, quote! { G: Generator<i64> });
}

#[test]
fn test_mut_arg_binding_stays_in_body_fn() {
    let out = expand(quote! {
        fn counted(tc: &TestCase, mut count: usize) -> usize {
            count += 1;
            count
        }
    });
    assert_contains_tokens(
        &out,
        quote! { fn counted(count: usize) -> CountedCompositeGenerator },
    );
    assert_contains_tokens(&out, quote! { mut count: usize });
}

#[test]
fn test_by_value_test_case_gets_migration_error() {
    let out = expand_error(quote! {
        fn tree(tc: TestCase) -> BinTree {
            tc.draw(gs::just(BinTree::Leaf()))
        }
    });
    assert!(
        out.contains("change `tc: TestCase` to `tc: &TestCase`"),
        "error should tell the user how to migrate: {out}"
    );
}

#[test]
fn test_mut_reference_test_case_is_rejected() {
    let out = expand_error(quote! {
        fn tree(tc: &mut TestCase) -> BinTree {
            tc.draw(gs::just(BinTree::Leaf()))
        }
    });
    assert!(
        out.contains("change `&mut TestCase` to `&TestCase`"),
        "error should point at the mutable reference: {out}"
    );
}

#[test]
fn test_missing_first_parameter_is_rejected() {
    let out = expand_error(quote! {
        fn tree() -> BinTree {
            BinTree::Leaf()
        }
    });
    assert!(
        out.contains("first parameter of type &TestCase"),
        "error should describe the required first parameter: {out}"
    );
}

#[test]
fn test_missing_return_type_is_rejected() {
    let out = expand_error(quote! {
        fn tree(tc: &TestCase) {
            tc.draw(gs::booleans());
        }
    });
    assert!(
        out.contains("explicitly declare a return type"),
        "error should require a return type: {out}"
    );
}

#[test]
fn test_non_identifier_arg_pattern_is_rejected() {
    let out = expand_error(quote! {
        fn bounded(tc: &TestCase, (lo, hi): (i32, i32)) -> i32 {
            tc.draw(gs::integers::<i32>().min_value(lo).max_value(hi))
        }
    });
    assert!(
        out.contains("plain identifiers"),
        "error should require identifier parameters: {out}"
    );
}
