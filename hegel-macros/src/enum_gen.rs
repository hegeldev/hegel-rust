use std::collections::HashMap;

use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{DeriveInput, Fields, Variant};

use crate::utils::{
    GenericsParts, default_gen_bounds, generator_param_ident, is_valid_method_name,
    make_method_ident, pascal_to_snake, print_shape, split_generics,
};

/// Extract all field types from a variant.
fn variant_field_types(variant: &Variant) -> Vec<&syn::Type> {
    variant.fields.iter().map(|f| &f.ty).collect()
}

/// Derive Generator for an enum.
pub(crate) fn derive_enum_generator(input: &DeriveInput, data: &syn::DataEnum) -> TokenStream {
    let enum_name = &input.ident;
    let generator_name = format_ident!("{}Generator", enum_name);

    if data.variants.is_empty() {
        return syn::Error::new_spanned(
            enum_name,
            "DefaultGenerator cannot be derived for enums with no variants: \
             there is no value to generate",
        )
        .to_compile_error()
        .into();
    }

    let GenericsParts {
        gen_params,
        param_uses,
        type_param_idents,
        user_predicates,
        ty_generics,
    } = match split_generics(&input.generics) {
        Ok(parts) => parts,
        Err(err) => return err.to_compile_error().into(),
    };
    let self_ty = quote! { #enum_name #ty_generics };
    let gen_params = if gen_params.is_empty() {
        quote! {}
    } else {
        quote! { #gen_params, }
    };
    let has_generics = !input.generics.params.is_empty();
    let variant_phantom_field = if has_generics {
        quote! { __phantom: ::core::marker::PhantomData<fn() -> #self_ty>, }
    } else {
        quote! {}
    };
    let variant_phantom_init = if has_generics {
        quote! { __phantom: ::core::marker::PhantomData, }
    } else {
        quote! {}
    };

    let variants: Vec<_> = data.variants.iter().collect();
    let data_variants: Vec<_> = variants
        .iter()
        .filter(|v| !matches!(v.fields, Fields::Unit))
        .collect();

    let field_names: Vec<syn::Ident> = {
        let method_strs: Vec<String> = data_variants
            .iter()
            .map(|v| {
                let snake = pascal_to_snake(&v.ident.to_string());
                if is_valid_method_name(&snake) {
                    snake
                } else {
                    format!("{}_", snake)
                }
            })
            .collect();
        // A tuple variant also gets a `<name>_with` builder, so its
        // generated names can collide not just with another variant's base
        // name but with another variant's `_with` name (`Foo` + `FooWith`).
        // Count every generated name and fall back to the raw variant ident
        // on any collision.
        let mut counts: HashMap<String, usize> = HashMap::new();
        for (v, name) in data_variants.iter().zip(method_strs.iter()) {
            *counts.entry(name.clone()).or_insert(0) += 1;
            if matches!(v.fields, Fields::Unnamed(_)) {
                *counts.entry(format!("{name}_with")).or_insert(0) += 1;
            }
        }
        data_variants
            .iter()
            .zip(method_strs.iter())
            .map(|(v, name)| {
                let with_name = format!("{name}_with");
                let own_with = usize::from(matches!(v.fields, Fields::Unnamed(_)));
                let collides = counts[name.as_str()] > 1
                    || counts
                        .get(with_name.as_str())
                        .is_some_and(|&n| n > own_with);
                if collides {
                    v.ident.clone()
                } else {
                    make_method_ident(name, v.ident.span())
                }
            })
            .collect()
    };

    let variant_generators: Vec<_> = data_variants
        .iter()
        .map(|variant| {
            generate_variant_generator(
                enum_name,
                variant,
                &gen_params,
                &param_uses,
                &user_predicates,
                &self_ty,
                &variant_phantom_field,
                &variant_phantom_init,
            )
        })
        .collect();

    let variant_generator_names: Vec<_> = data_variants
        .iter()
        .map(|variant| format_ident!("{}{}Generator", enum_name, variant.ident))
        .collect();

    let variant_params: Vec<syn::Ident> = data_variants
        .iter()
        .map(|variant| format_ident!("__V{}", variant.ident))
        .collect();

    let variant_param_decls: Vec<_> = variant_params
        .iter()
        .zip(variant_generator_names.iter())
        .map(|(param, variant_generator_name)| {
            quote! {
                #param = #variant_generator_name<#(#param_uses,)*>
            }
        })
        .collect();

    let generator_fields: Vec<_> = field_names
        .iter()
        .zip(variant_params.iter())
        .map(|(field_name, param)| {
            quote! {
                #field_name: #param
            }
        })
        .collect();

    let new_field_inits: Vec<_> = field_names
        .iter()
        .zip(variant_generator_names.iter())
        .map(|(field_name, variant_generator_name)| {
            quote! {
                #field_name: #variant_generator_name::new()
            }
        })
        .collect();

    let default_bounds: Vec<_> = data_variants
        .iter()
        .flat_map(|variant| default_gen_bounds(&variant_field_types(variant)))
        .collect();

    let with_methods: Vec<_> = data_variants
        .iter()
        .enumerate()
        .map(|(index, variant)| {
            let field_name = &field_names[index];
            let variant_name = &variant.ident;
            let variant_generator_name = &variant_generator_names[index];
            let params_before = &variant_params[..index];
            let params_after = &variant_params[index + 1..];
            let moves = |replacement: proc_macro2::TokenStream| {
                let assignments: Vec<_> = field_names
                    .iter()
                    .enumerate()
                    .map(|(other, other_name)| {
                        if other == index {
                            quote! { #field_name: #replacement }
                        } else {
                            quote! { #other_name: self.#other_name }
                        }
                    })
                    .collect();
                assignments
            };

            match &variant.fields {
                Fields::Unit => unreachable!(),
                Fields::Named(_) => {
                    let field_types = variant_field_types(variant);
                    let bounds = default_gen_bounds(&field_types);
                    let assignments = moves(quote! { configure(#variant_generator_name::new()) });

                    let doc = format!("Set a custom generator for the `{variant_name}` variant.");
                    quote! {
                        #[doc = #doc]
                        pub fn #field_name<F, G>(
                            self,
                            configure: F,
                        ) -> #generator_name<#(#param_uses,)* #(#params_before,)* G, #(#params_after,)*>
                        where
                            F: FnOnce(#variant_generator_name<#(#param_uses,)*>) -> G,
                            G: ::hegel::generators::Generator<#self_ty>,
                            #(#bounds,)*
                        {
                            #generator_name {
                                #(#assignments,)*
                                _phantom: ::core::marker::PhantomData,
                            }
                        }
                    }
                }
                Fields::Unnamed(_) => {
                    let field_types = variant_field_types(variant);
                    let gen_type_params: Vec<_> = (0..field_types.len())
                        .map(|i| format_ident!("G{}", i))
                        .collect();
                    let gen_param_names: Vec<_> = (0..field_types.len())
                        .map(|i| format_ident!("gen_{}", i))
                        .collect();
                    let field_indices: Vec<_> = (0..field_types.len())
                        .map(|i| format_ident!("_{}", i))
                        .collect();
                    let bounds: Vec<_> = gen_type_params
                        .iter()
                        .zip(field_types.iter())
                        .map(|(gtp, ft)| {
                            quote! {
                                #gtp: ::hegel::generators::Generator<#ft>
                            }
                        })
                        .collect();
                    let replacement_ty = quote! {
                        #variant_generator_name<#(#param_uses,)* #(#gen_type_params,)*>
                    };
                    let assignments = moves(quote! {
                        #variant_generator_name {
                            #(#field_indices: #gen_param_names,)*
                            #variant_phantom_init
                        }
                    });

                    let with_method_name = format_ident!("{}_with", field_name);
                    let with_bounds = default_gen_bounds(&field_types);
                    let with_assignments =
                        moves(quote! { configure(#variant_generator_name::new()) });
                    let with_doc = format!(
                        "Configure the `{variant_name}` variant via a closure.\n\nThe closure \
                         receives the default variant generator and must return any generator \
                         producing `{enum_name}`."
                    );
                    let with_method = quote! {
                        #[doc = #with_doc]
                        pub fn #with_method_name<F, G>(
                            self,
                            configure: F,
                        ) -> #generator_name<#(#param_uses,)* #(#params_before,)* G, #(#params_after,)*>
                        where
                            F: FnOnce(#variant_generator_name<#(#param_uses,)*>) -> G,
                            G: ::hegel::generators::Generator<#self_ty>,
                            #(#with_bounds,)*
                        {
                            #generator_name {
                                #(#with_assignments,)*
                                _phantom: ::core::marker::PhantomData,
                            }
                        }
                    };

                    let doc = format!("Set custom generators for the `{variant_name}` variant.");
                    quote! {
                        #[doc = #doc]
                        pub fn #field_name<#(#gen_type_params),*>(
                            self,
                            #(#gen_param_names: #gen_type_params),*
                        ) -> #generator_name<#(#param_uses,)* #(#params_before,)* #replacement_ty, #(#params_after,)*>
                        where
                            #(#bounds,)*
                        {
                            #generator_name {
                                #(#assignments,)*
                                _phantom: ::core::marker::PhantomData,
                            }
                        }

                        #with_method
                    }
                }
            }
        })
        .collect();

    let max_variant_idx = variants.len() - 1;
    let variant_index_draw = quote! {
        ::hegel::generators::integers::<usize>()
            .min_value(0)
            .max_value(#max_variant_idx)
            .do_draw(__tc)
    };

    let variant_to_field: HashMap<String, &syn::Ident> = data_variants
        .iter()
        .zip(field_names.iter())
        .map(|(v, f)| (v.ident.to_string(), f))
        .collect();

    let generate_match_arms: Vec<_> = variants
        .iter()
        .enumerate()
        .map(|(i, variant)| {
            let variant_name = &variant.ident;
            match &variant.fields {
                Fields::Unit => quote! { #i => #enum_name::#variant_name },
                _ => {
                    let field_name = variant_to_field[&variant.ident.to_string()];
                    quote! { #i => self.#field_name.do_draw(__tc) }
                }
            }
        })
        .collect();

    let struct_doc = format!("Generated generator for `{enum_name}`.");
    let generator_struct = quote! {
        #[doc = #struct_doc]
        pub struct #generator_name<#gen_params #(#variant_param_decls,)*>
        where
            #(#user_predicates,)*
        {
            #(#generator_fields,)*
            _phantom: ::core::marker::PhantomData<fn() -> #self_ty>,
        }

        impl<#gen_params> #generator_name<#(#param_uses,)*>
        where
            #(#user_predicates,)*
            #(#default_bounds,)*
        {
            /// Create a new generator with default generators for all variants.
            pub fn new() -> Self {
                Self {
                    #(#new_field_inits,)*
                    _phantom: ::core::marker::PhantomData,
                }
            }
        }

        impl<#gen_params #(#variant_params,)*> #generator_name<#(#param_uses,)* #(#variant_params,)*>
        where
            #(#user_predicates,)*
        {
            #(#with_methods)*
        }

        impl<#gen_params> Default for #generator_name<#(#param_uses,)*>
        where
            #(#user_predicates,)*
            #(#default_bounds,)*
        {
            fn default() -> Self {
                Self::new()
            }
        }
    };

    let unit_variant_match_arms: Vec<proc_macro2::TokenStream> = variants
        .iter()
        .enumerate()
        .map(|(i, variant)| {
            let variant_name = &variant.ident;
            quote! { #i => #enum_name::#variant_name }
        })
        .collect();

    let print_match_arms: Vec<_> = variants
        .iter()
        .enumerate()
        .map(|(i, variant)| {
            let variant_name = &variant.ident;
            match &variant.fields {
                Fields::Unit => {
                    let label = format!("{enum_name}::{variant_name}");
                    quote! { #i => { __printer.text(#label); #enum_name::#variant_name } }
                }
                _ => {
                    let field_name = variant_to_field[&variant.ident.to_string()];
                    quote! { #i => self.#field_name.draw_and_print(__tc, __printer) }
                }
            }
        })
        .collect();

    let generate_trait_impl = if data_variants.is_empty() {
        quote! {
            impl<#gen_params> ::hegel::generators::Generator<#self_ty>
                for #generator_name<#(#param_uses,)*>
            where
                #(#user_predicates,)*
            {
                fn do_draw(&self, __tc: &::hegel::TestCase) -> #self_ty {
                    let index: usize = #variant_index_draw;
                    match index {
                        #(#unit_variant_match_arms,)*
                        _ => unreachable!("Unknown variant index: {}", index),
                    }
                }
            }

            impl<#gen_params> ::hegel::generators::PrintableGenerator<#self_ty>
                for #generator_name<#(#param_uses,)*>
            where
                #(#user_predicates,)*
            {
                fn do_draw_and_print(
                    &self,
                    __tc: &::hegel::TestCase,
                    __printer: &mut ::hegel::PrettyPrinter,
                ) -> #self_ty {
                    let index: usize = #variant_index_draw;
                    match index {
                        #(#print_match_arms,)*
                        _ => unreachable!("Unknown variant index: {}", index),
                    }
                }
            }
        }
    } else {
        quote! {
            impl<#gen_params #(#variant_params,)*> ::hegel::generators::Generator<#self_ty>
                for #generator_name<#(#param_uses,)* #(#variant_params,)*>
            where
                #(#user_predicates,)*
                #(#variant_params: ::hegel::generators::Generator<#self_ty>,)*
            {
                fn do_draw(&self, __tc: &::hegel::TestCase) -> #self_ty {
                    __tc.start_span(::hegel::generators::labels::ENUM_VARIANT);
                    let index: usize = #variant_index_draw;

                    let __result = match index {
                        #(#generate_match_arms,)*
                        _ => unreachable!("Unknown variant index: {}", index),
                    };
                    __tc.stop_span(false);
                    __result
                }
            }

            impl<#gen_params #(#variant_params,)*> ::hegel::generators::PrintableGenerator<#self_ty>
                for #generator_name<#(#param_uses,)* #(#variant_params,)*>
            where
                #(#user_predicates,)*
                #(#variant_params: ::hegel::generators::PrintableGenerator<#self_ty>,)*
            {
                fn do_draw_and_print(
                    &self,
                    __tc: &::hegel::TestCase,
                    __printer: &mut ::hegel::PrettyPrinter,
                ) -> #self_ty {
                    __tc.start_span(::hegel::generators::labels::ENUM_VARIANT);
                    let index: usize = #variant_index_draw;

                    let __result = match index {
                        #(#print_match_arms,)*
                        _ => unreachable!("Unknown variant index: {}", index),
                    };
                    __tc.stop_span(false);
                    __result
                }
            }
        }
    };

    let default_generator_impl = quote! {
        impl<#gen_params> ::hegel::generators::DefaultGenerator for #self_ty
        where
            #(#user_predicates,)*
            #(#type_param_idents: 'static,)*
            #(#default_bounds,)*
        {
            type Generator = #generator_name<#(#param_uses,)*>;
            fn default_generator() -> Self::Generator {
                #generator_name::new()
            }
        }
    };

    let expanded = quote! {
        #[allow(non_camel_case_types, non_snake_case)]
        const _: () = {
            use ::hegel::generators::Generator as _;
            use ::hegel::generators::PrintableGenerator as _;

            #(#variant_generators)*

            #generator_struct

            #generate_trait_impl

            #default_generator_impl
        };
    };

    TokenStream::from(expanded)
}

/// Generate a variant generator struct for a data variant.
#[allow(clippy::too_many_arguments)]
fn generate_variant_generator(
    enum_name: &syn::Ident,
    variant: &Variant,
    gen_params: &proc_macro2::TokenStream,
    param_uses: &[proc_macro2::TokenStream],
    user_predicates: &[&syn::WherePredicate],
    self_ty: &proc_macro2::TokenStream,
    phantom_field: &proc_macro2::TokenStream,
    phantom_init: &proc_macro2::TokenStream,
) -> proc_macro2::TokenStream {
    let variant_name = &variant.ident;
    let variant_generator_name = format_ident!("{}{}Generator", enum_name, variant_name);
    let struct_doc =
        format!("Generated generator for the `{variant_name}` variant of `{enum_name}`.");

    let (field_idents, construction_shape): (Vec<syn::Ident>, _) = match &variant.fields {
        Fields::Unit => return quote! {},
        Fields::Named(fields) => {
            let field_names: Vec<syn::Ident> = fields
                .named
                .iter()
                .map(|f| f.ident.as_ref().unwrap().clone())
                .collect();
            for field_name in &field_names {
                if *field_name == "new" || *field_name == "boxed" || *field_name == "__phantom" {
                    return syn::Error::new_spanned(
                        field_name,
                        format!(
                            "field name `{field_name}` collides with the generated builder API \
                             of #[derive(DefaultGenerator)]; rename the field or implement \
                             DefaultGenerator by hand"
                        ),
                    )
                    .to_compile_error();
                }
            }
            (field_names, true)
        }
        Fields::Unnamed(fields) => {
            let field_indices: Vec<syn::Ident> = (0..fields.unnamed.len())
                .map(|i| format_ident!("_{}", i))
                .collect();
            (field_indices, false)
        }
    };

    let field_types = variant_field_types(variant);
    let mut generator_params: Vec<syn::Ident> = field_idents
        .iter()
        .map(|ident| generator_param_ident(&ident.to_string()))
        .collect();
    {
        let mut seen = std::collections::HashSet::new();
        if generator_params
            .iter()
            .any(|param| !seen.insert(param.to_string()))
        {
            generator_params = (0..field_idents.len())
                .map(|i| format_ident!("__G{}", i))
                .collect();
        }
    }

    let generator_param_decls: Vec<_> = generator_params
        .iter()
        .zip(field_types.iter())
        .map(|(param, ty)| {
            quote! {
                #param = <#ty as ::hegel::generators::DefaultGenerator>::Generator
            }
        })
        .collect();

    let generator_fields: Vec<_> = field_idents
        .iter()
        .zip(generator_params.iter())
        .map(|(ident, param)| {
            quote! { #ident: #param }
        })
        .collect();

    let new_inits: Vec<_> = field_idents
        .iter()
        .zip(field_types.iter())
        .map(|(ident, ty)| {
            quote! {
                #ident: <#ty as ::hegel::generators::DefaultGenerator>::default_generator()
            }
        })
        .collect();

    let default_bounds = default_gen_bounds(&field_types);

    let builder_methods: Vec<_> = field_idents
        .iter()
        .enumerate()
        .map(|(index, field_ident)| {
            let field_type = field_types[index];
            let params_before = &generator_params[..index];
            let params_after = &generator_params[index + 1..];
            let moves = field_idents.iter().enumerate().map(|(other, other_ident)| {
                if other == index {
                    quote! { #field_ident: generator }
                } else {
                    quote! { #other_ident: self.#other_ident }
                }
            });
            quote! {
                /// Set a custom generator for this field.
                ///
                /// Any [`Generator`](::hegel::generators::Generator) of the
                /// field's type is accepted; the resulting generator can be
                /// passed to [`draw`](::hegel::TestCase::draw) exactly when
                /// every field generator is a
                /// [`PrintableGenerator`](::hegel::generators::PrintableGenerator).
                pub fn #field_ident<G>(
                    self,
                    generator: G,
                ) -> #variant_generator_name<#(#param_uses,)* #(#params_before,)* G, #(#params_after,)*>
                where
                    G: ::hegel::generators::Generator<#field_type>,
                {
                    #variant_generator_name {
                        #(#moves,)*
                        #phantom_init
                    }
                }
            }
        })
        .collect();

    let (construction, print_construction, print_actions) = if construction_shape {
        let field_constructions: Vec<_> = field_idents
            .iter()
            .map(|ident| {
                quote! { #ident: self.#ident.do_draw(__tc) }
            })
            .collect();
        let print_idents: Vec<_> = (0..field_idents.len())
            .map(|i| format_ident!("__field{i}"))
            .collect();
        let print_actions: Vec<_> = print_idents
            .iter()
            .zip(field_idents.iter())
            .map(|(print_ident, field_ident)| {
                quote! {
                    let #print_ident = self.#field_ident.draw_and_print(__tc, __printer);
                }
            })
            .collect();
        (
            quote! { #enum_name::#variant_name { #(#field_constructions,)* } },
            quote! { #enum_name::#variant_name { #(#field_idents: #print_idents,)* } },
            print_actions,
        )
    } else {
        let field_generates: Vec<_> = field_idents
            .iter()
            .map(|ident| {
                quote! { self.#ident.do_draw(__tc) }
            })
            .collect();
        let print_idents: Vec<_> = (0..field_idents.len())
            .map(|i| format_ident!("__field{i}"))
            .collect();
        let print_actions: Vec<_> = print_idents
            .iter()
            .zip(field_idents.iter())
            .map(|(print_ident, field_ident)| {
                quote! {
                    let #print_ident = self.#field_ident.draw_and_print(__tc, __printer);
                }
            })
            .collect();
        (
            quote! { #enum_name::#variant_name(#(#field_generates,)*) },
            quote! { #enum_name::#variant_name(#(#print_idents,)*) },
            print_actions,
        )
    };

    let label = format!("{enum_name}::{variant_name}");
    let print_body = print_shape(&label, &variant.fields, &print_actions);

    quote! {
        #[doc = #struct_doc]
        pub struct #variant_generator_name<#gen_params #(#generator_param_decls,)*>
        where
            #(#user_predicates,)*
        {
            #(#generator_fields,)*
            #phantom_field
        }

        impl<#gen_params> #variant_generator_name<#(#param_uses,)*>
        where
            #(#user_predicates,)*
            #(#default_bounds,)*
        {
            /// Create a new generator with default generators for all fields.
            pub fn new() -> Self {
                Self {
                    #(#new_inits,)*
                    #phantom_init
                }
            }
        }

        impl<#gen_params #(#generator_params,)*>
            #variant_generator_name<#(#param_uses,)* #(#generator_params,)*>
        where
            #(#user_predicates,)*
        {
            #(#builder_methods)*
        }

        impl<#gen_params> Default for #variant_generator_name<#(#param_uses,)*>
        where
            #(#user_predicates,)*
            #(#default_bounds,)*
        {
            fn default() -> Self {
                Self::new()
            }
        }

        impl<#gen_params #(#generator_params,)*> ::hegel::generators::Generator<#self_ty>
            for #variant_generator_name<#(#param_uses,)* #(#generator_params,)*>
        where
            #(#user_predicates,)*
            #(#generator_params: ::hegel::generators::Generator<#field_types>,)*
        {
            fn do_draw(&self, __tc: &::hegel::TestCase) -> #self_ty {
                #construction
            }
        }

        impl<#gen_params #(#generator_params,)*> ::hegel::generators::PrintableGenerator<#self_ty>
            for #variant_generator_name<#(#param_uses,)* #(#generator_params,)*>
        where
            #(#user_predicates,)*
            #(#generator_params: ::hegel::generators::PrintableGenerator<#field_types>,)*
        {
            fn do_draw_and_print(
                &self,
                __tc: &::hegel::TestCase,
                __printer: &mut ::hegel::PrettyPrinter,
            ) -> #self_ty {
                #print_body
                #print_construction
            }
        }
    }
}
