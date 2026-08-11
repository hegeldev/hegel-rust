use proc_macro2::TokenStream;
use quote::{ToTokens, format_ident, quote};
use syn::spanned::Spanned;
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
    let mut errors: Vec<syn::Error> = Vec::new();

    let input_parameters: Vec<FnArg> = f.sig.inputs.iter().cloned().collect();

    let (tc_arg, passthrough) = match input_parameters.split_first() {
        Some((FnArg::Typed(tc_arg), passthrough)) => (Some(tc_arg), passthrough),
        _ => {
            errors.push(syn::Error::new_spanned(
                &f.sig,
                "A #[composite] generator must define a first parameter of type &TestCase. When \
                    drawing from a #[composite] generator with tc.draw(my_composite_gen), the test \
                    case will be automatically passed to my_composite_gen as the first argument.",
            ));
            (None, [].as_slice())
        }
    };

    let (tc_inner_type, tc_type_is_valid) = match tc_arg.map(|arg| arg.ty.as_ref()) {
        Some(Type::Reference(reference)) if reference.mutability.is_none() => {
            (reference.elem.as_ref().clone(), true)
        }
        Some(Type::Reference(reference)) => {
            errors.push(syn::Error::new_spanned(
                reference,
                "A #[composite] generator borrows its TestCase through a shared reference: \
                change `&mut TestCase` to `&TestCase`. All TestCase methods take `&self`.",
            ));
            (parse_quote! { ::hegel::TestCase }, false)
        }
        Some(arg) => {
            errors.push(syn::Error::new_spanned(
                arg,
                "#[composite] generators receive the test case by reference: change \
                `tc: TestCase` to `tc: &TestCase`. (Earlier versions took the TestCase by \
                value; the body of the generator rarely needs any other change.)",
            ));
            (parse_quote! { ::hegel::TestCase }, false)
        }
        None => (parse_quote! { ::hegel::TestCase }, false),
    };

    let tc_arg: FnArg = match tc_arg {
        Some(arg) if tc_type_is_valid => FnArg::Typed(arg.clone()),
        Some(arg) => {
            let pattern = arg.pat.as_ref();
            parse_quote! { #pattern: &::hegel::TestCase }
        }
        None => parse_quote! { __hegel_tc: &::hegel::TestCase },
    };

    let return_type = if let ReturnType::Type(_, ty) = f.sig.output {
        ty
    } else {
        errors.push(syn::Error::new_spanned(
            &f.sig,
            "#[composite] generators must explicitly declare a return type.",
        ));
        parse_quote! { () }
    };

    let mut field_idents = Vec::new();
    let mut field_types = Vec::new();
    let mut body_parameters: Vec<FnArg> = Vec::new();
    for (index, arg) in passthrough.iter().enumerate() {
        let FnArg::Typed(typed) = arg else {
            unreachable!("syn only parses a receiver as the first parameter")
        };
        field_types.push(typed.ty.as_ref().clone());
        match typed.pat.as_ref() {
            Pat::Ident(pat) if pat.by_ref.is_none() && pat.subpat.is_none() => {
                field_idents.push(pat.ident.clone());
                body_parameters.push(arg.clone());
            }
            pattern => {
                errors.push(syn::Error::new_spanned(
                    pattern,
                    "parameters of a #[composite] generator (after the TestCase) must be plain \
                    identifiers, so they can be stored on the generated generator struct.",
                ));
                let ident = format_ident!("__hegel_arg{index}", span = pattern.span());
                let ty = typed.ty.as_ref();
                body_parameters.push(parse_quote! { #ident: #ty });
                field_idents.push(ident);
            }
        }
    }

    let fn_name = &f.sig.ident;
    let pascal_name = snake_to_pascal(fn_name.to_string().trim_start_matches("r#"));
    let struct_name = format_ident!("{pascal_name}CompositeGenerator", span = fn_name.span());
    let struct_doc = format!("Generator returned by [`{fn_name}()`].");

    let mut generics = f.sig.generics.clone();
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
    let errors = errors.into_iter().map(|e| e.to_compile_error());

    quote! {
        #(#errors)*

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
