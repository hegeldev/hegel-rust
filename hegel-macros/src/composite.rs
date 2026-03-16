use proc_macro2::TokenStream;
use quote::quote;
use syn::{parse2, FnArg, ItemFn, ReturnType};
use syn::token::Comma;
use syn::punctuated::Punctuated;

const MISSING_TEST_CASE_PARAMETER: &str = "Functions marked with #[hegel::composite] must have a TestCase parameter as their first argument.";
const MISSING_COMPOSITE_RETURN_TYPE: &str = "Functions marked with #[hegel::composite] must have an explicit return type.";

// Our goal is to expand this
//
// #[hegel::composite]
// fn composite_generator(tc: TestCase, a: A, b: B) -> C {
//     body
// }
//
// into this
//
// fn composite_generator(a: A, b: B) -> ComposedGenerator<C, impl Fn(TestCase) -> C> {
//     compose!(|tc| { body })
// }

pub fn expand_composite(
    f: ItemFn,
) -> TokenStream {

    // Clone the input parameters into a vector, so we can pull the first one out.
    let input_parameters: Vec<FnArg> = f.sig.inputs.iter().cloned().collect();

    let Some((FnArg::Typed(tc_arg), passthrough)) = input_parameters.split_first() else {
        panic!("{}", MISSING_TEST_CASE_PARAMETER)
    };
    let tc_pattern = &tc_arg.pat;

    let ReturnType::Type(_, return_type) = &f.sig.output else {
        panic!("{}", MISSING_COMPOSITE_RETURN_TYPE)
    };

    let composed_generator_type = quote! {
        -> ::hegel::generators::ComposedGenerator<#return_type, impl Fn(::hegel::TestCase) -> #return_type>
    };

    let mut signature = f.sig;
    signature.output = parse2(composed_generator_type).unwrap();
    signature.inputs = passthrough
        .iter()
        .cloned()
        .collect::<Punctuated<FnArg, Comma>>();

    let body = &f.block;
    let attributes = &f.attrs;
    let visibility = &f.vis;

    quote! {
        #(#attributes)*
        #visibility #signature
        { ::hegel::compose!(|#tc_pattern| #body) }
    }
}
