use proc_macro2::TokenStream;
use quote::{ToTokens, format_ident, quote};
use syn::{FnArg, ItemFn, Pat, ReturnType, Type, parse_quote};

use crate::utils::snake_to_pascal;

/// Replaces any elided lifetimes with the given lifetime
struct ElidedLifetimeRewriter {
    lifetime: syn::Lifetime,
    used: bool,
}
impl syn::visit_mut::VisitMut for ElidedLifetimeRewriter {
    fn visit_type_reference_mut(&mut self, node: &mut syn::TypeReference) {
        if node.lifetime.is_none() {
            node.lifetime = Some(self.lifetime.clone());
            self.used = true;
        }
        syn::visit_mut::visit_type_reference_mut(self, node);
    }
}

pub fn expand_composite(f: ItemFn) -> TokenStream {
    let input_parameters: Vec<FnArg> = f.sig.inputs.iter().cloned().collect();

    let Some((FnArg::Typed(tc_arg), passthrough)) = input_parameters.split_first() else {
        return syn::Error::new_spanned(
            &f.sig,
            "A #[composite] generator must define a first parameter of type &TestCase. When \
            drawing from a #[composite] generator with tc.draw(my_composite_gen), the test \
            case will be automatically passed to my_composite_gen as the first argument.",
        )
        .to_compile_error();
    };

    let tc_inner_type = match tc_arg.ty.as_ref() {
        Type::Reference(reference) if reference.mutability.is_none() => reference.elem.clone(),
        Type::Reference(reference) => {
            return syn::Error::new_spanned(
                reference,
                "A #[composite] generator borrows its TestCase through a shared reference: \
                change `&mut TestCase` to `&TestCase`. All TestCase methods take `&self`.",
            )
            .to_compile_error();
        }
        _ => {
            return syn::Error::new_spanned(
                &tc_arg.ty,
                "#[composite] generators receive the test case by reference: change \
                `tc: TestCase` to `tc: &TestCase`. (Earlier versions took the TestCase by \
                value; the body of the generator rarely needs any other change.)",
            )
            .to_compile_error();
        }
    };

    let ReturnType::Type(_, return_type) = &f.sig.output else {
        return syn::Error::new_spanned(
            &f.sig,
            "#[composite] generators must explicitly declare a return type.",
        )
        .to_compile_error();
    };

    let mut field_idents = Vec::new();
    let mut field_types = Vec::new();
    for arg in passthrough {
        let ident = match arg {
            FnArg::Typed(typed) => match typed.pat.as_ref() {
                Pat::Ident(pat) if pat.by_ref.is_none() && pat.subpat.is_none() => {
                    field_types.push(typed.ty.as_ref().clone());
                    pat.ident.clone()
                }
                _ => {
                    return syn::Error::new_spanned(
                        &typed.pat,
                        "parameters of a #[composite] generator (after the TestCase) must be \
                        plain identifiers, so they can be stored on the generated generator \
                        struct.",
                    )
                    .to_compile_error();
                }
            },
            FnArg::Receiver(_) => {
                unreachable!("syn only parses a receiver as the first parameter")
            }
        };
        field_idents.push(ident);
    }

    let fn_name = &f.sig.ident;
    let pascal_name = snake_to_pascal(fn_name.to_string().trim_start_matches("r#"));
    let struct_name = format_ident!("{pascal_name}CompositeGenerator", span = fn_name.span());
    let struct_doc = format!("Generator returned by [`{fn_name}()`].");

    let mut generics = f.sig.generics.clone();
    let mut body_parameters: Vec<FnArg> = passthrough.to_vec();
    {
        let mut rewriter = ElidedLifetimeRewriter {
            lifetime: syn::Lifetime::new("'__hegel_composite", fn_name.span()),
            used: false,
        };
        for ty in &mut field_types {
            syn::visit_mut::VisitMut::visit_type_mut(&mut rewriter, ty);
        }
        for arg in &mut body_parameters {
            syn::visit_mut::VisitMut::visit_fn_arg_mut(&mut rewriter, arg);
        }
        if rewriter.used {
            let lifetime = rewriter.lifetime;
            generics.params.insert(0, parse_quote! { #lifetime });
        }
    }

    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    let lifetimes: Vec<_> = generics.lifetimes().map(|l| &l.lifetime).collect();
    let type_params: Vec<_> = generics.type_params().map(|t| &t.ident).collect();
    let has_marker = !lifetimes.is_empty() || !type_params.is_empty();
    let marker_field = has_marker.then(|| {
        quote! {
            __hegel_phantom: ::core::marker::PhantomData<(#(&#lifetimes (),)* fn() -> (#(#type_params,)*))>,
        }
    });
    let marker_init = has_marker.then(|| quote! { __hegel_phantom: ::core::marker::PhantomData, });

    let user_predicates: Vec<_> = generics
        .where_clause
        .as_ref()
        .map(|w| w.predicates.iter().collect())
        .unwrap_or_default();
    let clone_bounds: Vec<_> = field_types
        .iter()
        .map(|ty| quote! { #ty: ::core::clone::Clone })
        .collect();
    let generator_where = if clone_bounds.is_empty() && user_predicates.is_empty() {
        quote! {}
    } else {
        quote! { where #(#clone_bounds,)* #(#user_predicates,)* }
    };

    let label_source = f.block.to_token_stream().to_string();
    let inner_block = f.block;
    let body_block: syn::Block = parse_quote! {{
        ::hegel::__assert_is_test_case::< #tc_inner_type >();
        #inner_block
    }};

    let attributes = &f.attrs;
    let visibility = &f.vis;

    quote! {
        #[doc = #struct_doc]
        #visibility struct #struct_name #generics #where_clause {
            #(#field_idents: #field_types,)*
            #marker_field
        }

        #(#attributes)*
        #visibility fn #fn_name #generics (#(#field_idents: #field_types),*) -> #struct_name #ty_generics #where_clause {
            #struct_name {
                #(#field_idents,)*
                #marker_init
            }
        }

        impl #impl_generics #struct_name #ty_generics #where_clause {
            fn __hegel_body(#tc_arg, #(#body_parameters),*) -> #return_type #body_block
        }

        impl #impl_generics ::core::clone::Clone for #struct_name #ty_generics #generator_where {
            fn clone(&self) -> Self {
                Self {
                    #(#field_idents: ::core::clone::Clone::clone(&self.#field_idents),)*
                    #marker_init
                }
            }
        }

        impl #impl_generics ::hegel::generators::Generator<#return_type> for #struct_name #ty_generics #generator_where {
            fn do_draw(&self, tc: &::hegel::TestCase) -> #return_type {
                const __HEGEL_COMPOSITE_LABEL: u64 =
                    ::hegel::generators::fnv1a_hash(#label_source.as_bytes());
                tc.start_span(__HEGEL_COMPOSITE_LABEL);
                let __hegel_result =
                    Self::__hegel_body(tc, #(::core::clone::Clone::clone(&self.#field_idents)),*);
                tc.stop_span(false);
                __hegel_result
            }
        }
    }
}

#[cfg(test)]
#[path = "../tests/embedded/composite_tests.rs"]
mod tests;
