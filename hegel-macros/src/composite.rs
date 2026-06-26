use proc_macro2::TokenStream;
use quote::quote;
use syn::punctuated::Punctuated;
use syn::token::Comma;
use syn::{FnArg, ItemFn, ReturnType, parse_quote, parse2};

pub fn expand_composite(mut f: ItemFn) -> TokenStream {
    let input_parameters: Vec<FnArg> = f.sig.inputs.iter().cloned().collect();

    let Some((FnArg::Typed(tc_arg), passthrough)) = input_parameters.split_first() else {
        panic!(
            "A #[composite] generator must define a first parameter of type TestCase. When \
            drawing from a #[composite] generator with tc.draw(my_composite_gen), `tc` will \
            be automatically passed to my_composite_gen as the first argument."
        )
    };
    let tc_pattern = &tc_arg.pat;
    let tc_type = &tc_arg.ty;

    let ReturnType::Type(_, return_type) = &f.sig.output else {
        panic!("#[composite] generators must explicitly declare a return type.")
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

    f.block.stmts.insert(
        0,
        parse_quote! {
            ::hegel::__assert_is_test_case::< #tc_type >();
        },
    );

    let body = &f.block;
    let attributes = &f.attrs;
    let visibility = &f.vis;

    quote! {
        #(#attributes)*
        #visibility #signature
        { ::hegel::compose!(|#tc_pattern| #body) }
    }
}
