use std::collections::HashMap;

use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{DeriveInput, Fields, Variant};

use crate::utils::{default_gen_bounds, is_valid_method_name, make_method_ident, pascal_to_snake};

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
        .map(|variant| generate_variant_generator(enum_name, variant))
        .collect();

    let generator_fields: Vec<_> = field_names
        .iter()
        .map(|field_name| {
            quote! {
                pub #field_name: ::hegel::generators::BoxedGenerator<'a, #enum_name>
            }
        })
        .collect();

    let new_field_inits: Vec<_> = data_variants
        .iter()
        .zip(field_names.iter())
        .map(|(variant, field_name)| {
            let variant_name = &variant.ident;
            let variant_generator_name = format_ident!("{}{}Generator", enum_name, variant_name);

            quote! {
                #field_name: #variant_generator_name::new().boxed()
            }
        })
        .collect();

    let default_bounds: Vec<_> = data_variants
        .iter()
        .flat_map(|variant| default_gen_bounds(&variant_field_types(variant), quote! { 'a }))
        .collect();

    let with_methods: Vec<_> = data_variants
        .iter()
        .zip(field_names.iter())
        .map(|(variant, field_name)| {
            let variant_name = &variant.ident;
            let variant_generator_name = format_ident!("{}{}Generator", enum_name, variant_name);

            match &variant.fields {
                Fields::Unit => unreachable!(),
                Fields::Named(_) => {
                    let field_types = variant_field_types(variant);
                    let bounds = default_gen_bounds(&field_types, quote! { 'a });

                    let doc = format!("Set a custom generator for the `{variant_name}` variant.");
                    quote! {
                        #[doc = #doc]
                        pub fn #field_name<F, G>(mut self, configure: F) -> Self
                        where
                            F: FnOnce(#variant_generator_name<'a>) -> G,
                            G: ::hegel::generators::Generator<#enum_name> + Send + Sync + 'a,
                            #(#bounds,)*
                        {
                            self.#field_name = configure(#variant_generator_name::new()).boxed();
                            self
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
                                #gtp: ::hegel::generators::Generator<#ft> + Send + Sync + 'a
                            }
                        })
                        .collect();

                    let with_method_name = format_ident!("{}_with", field_name);
                    let with_bounds = default_gen_bounds(&field_types, quote! { 'a });
                    let with_doc = format!(
                        "Configure the `{variant_name}` variant via a closure.\n\nThe closure \
                         receives the default variant generator and must return any generator \
                         producing `{enum_name}`."
                    );
                    let with_method = quote! {
                        #[doc = #with_doc]
                        pub fn #with_method_name<F, G>(mut self, configure: F) -> Self
                        where
                            F: FnOnce(#variant_generator_name<'a>) -> G,
                            G: ::hegel::generators::Generator<#enum_name> + Send + Sync + 'a,
                            #(#with_bounds,)*
                        {
                            self.#field_name = configure(#variant_generator_name::new()).boxed();
                            self
                        }
                    };

                    let doc = format!("Set custom generators for the `{variant_name}` variant.");
                    quote! {
                        #[doc = #doc]
                        pub fn #field_name<#(#gen_type_params),*>(
                            mut self,
                            #(#gen_param_names: #gen_type_params),*
                        ) -> Self
                        where
                            #(#bounds,)*
                        {
                            self.#field_name = #variant_generator_name {
                                #(#field_indices: #gen_param_names.boxed(),)*
                            }.boxed();
                            self
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
        pub struct #generator_name<'a> {
            #(#generator_fields,)*
            _phantom: ::core::marker::PhantomData<&'a ()>,
        }

        impl<'a> #generator_name<'a> {
            /// Create a new generator with default generators for all variants.
            pub fn new() -> Self
            where
                #(#default_bounds,)*
            {
                Self {
                    #(#new_field_inits,)*
                    _phantom: ::core::marker::PhantomData,
                }
            }

            #(#with_methods)*
        }

        impl<'a> Default for #generator_name<'a>
        where
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

    let generate_trait_impl = if data_variants.is_empty() {
        quote! {
            impl<'a> ::hegel::generators::Generator<#enum_name> for #generator_name<'a> {
                fn do_draw(&self, __tc: &::hegel::TestCase) -> #enum_name {
                    let index: usize = #variant_index_draw;
                    match index {
                        #(#unit_variant_match_arms,)*
                        _ => unreachable!("Unknown variant index: {}", index),
                    }
                }
            }
        }
    } else {
        quote! {
            impl<'a> ::hegel::generators::Generator<#enum_name> for #generator_name<'a> {
                fn do_draw(&self, __tc: &::hegel::TestCase) -> #enum_name {
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
        }
    };

    let default_generator_bounds: Vec<_> = data_variants
        .iter()
        .flat_map(|variant| default_gen_bounds(&variant_field_types(variant), quote! { 'static }))
        .collect();

    let default_generator_impl = quote! {
        impl ::hegel::generators::DefaultGenerator for #enum_name
        where
            #(#default_generator_bounds,)*
        {
            type Generator = #generator_name<'static>;
            fn default_generator() -> Self::Generator {
                #generator_name::new()
            }
        }
    };

    let expanded = quote! {
        #[allow(non_camel_case_types, non_snake_case)]
        const _: () = {
            use ::hegel::generators::Generator as _;

            #(#variant_generators)*

            #generator_struct

            #generate_trait_impl

            #default_generator_impl
        };
    };

    TokenStream::from(expanded)
}

/// Generate a variant generator struct for a data variant.
fn generate_variant_generator(
    enum_name: &syn::Ident,
    variant: &Variant,
) -> proc_macro2::TokenStream {
    let variant_name = &variant.ident;
    let variant_generator_name = format_ident!("{}{}Generator", enum_name, variant_name);
    let struct_doc =
        format!("Generated generator for the `{variant_name}` variant of `{enum_name}`.");

    match &variant.fields {
        Fields::Unit => {
            quote! {}
        }
        Fields::Named(fields) => {
            let field_names: Vec<_> = fields
                .named
                .iter()
                .map(|f| f.ident.as_ref().unwrap())
                .collect();
            for field_name in &field_names {
                if *field_name == "new" || *field_name == "boxed" {
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
            let field_types: Vec<_> = fields.named.iter().map(|f| &f.ty).collect();
            let builder_methods: Vec<_> = field_names
                .iter()
                .zip(field_types.iter())
                .map(|(field_name, field_type)| {
                    quote! {
                        /// Set a custom generator for this field.
                        pub fn #field_name<G>(mut self, generator: G) -> Self
                        where
                            G: ::hegel::generators::Generator<#field_type> + Send + Sync + 'a,
                        {
                            self.#field_name = generator.boxed();
                            self
                        }
                    }
                })
                .collect();

            let generator_fields: Vec<_> = field_names
                .iter()
                .zip(field_types.iter())
                .map(|(field_name, field_type)| {
                    quote! { #field_name: ::hegel::generators::BoxedGenerator<'a, #field_type> }
                })
                .collect();

            let new_inits: Vec<_> = field_names
                .iter()
                .zip(field_types.iter())
                .map(|(field_name, field_type)| {
                    quote! {
                        #field_name: <#field_type as ::hegel::generators::DefaultGenerator>::default_generator().boxed()
                    }
                })
                .collect();

            let default_bounds = default_gen_bounds(&field_types, quote! { 'a });

            let field_constructions: Vec<_> = field_names
                .iter()
                .map(|field_name| {
                    quote! { #field_name: self.#field_name.do_draw(__tc) }
                })
                .collect();

            quote! {
                #[doc = #struct_doc]
                pub struct #variant_generator_name<'a> {
                    #(#generator_fields,)*
                }

                impl<'a> #variant_generator_name<'a> {
                    /// Create a new generator with default generators for all fields.
                    pub fn new() -> Self
                    where
                        #(#default_bounds,)*
                    {
                        Self {
                            #(#new_inits,)*
                        }
                    }

                    #(#builder_methods)*
                }

                impl<'a> Default for #variant_generator_name<'a>
                where
                    #(#default_bounds,)*
                {
                    fn default() -> Self {
                        Self::new()
                    }
                }

                impl<'a> ::hegel::generators::Generator<#enum_name> for #variant_generator_name<'a> {
                    fn do_draw(&self, __tc: &::hegel::TestCase) -> #enum_name {
                        #enum_name::#variant_name {
                            #(#field_constructions,)*
                        }
                    }
                }
            }
        }
        Fields::Unnamed(fields) => {
            let field_types: Vec<_> = fields.unnamed.iter().map(|f| &f.ty).collect();
            let field_indices: Vec<_> = (0..field_types.len())
                .map(|i| format_ident!("_{}", i))
                .collect();

            let generator_fields: Vec<_> = field_indices
                .iter()
                .zip(field_types.iter())
                .map(|(field_idx, field_type)| {
                    quote! { #field_idx: ::hegel::generators::BoxedGenerator<'a, #field_type> }
                })
                .collect();

            let new_inits: Vec<_> = field_indices
                .iter()
                .zip(field_types.iter())
                .map(|(field_idx, field_type)| {
                    quote! {
                        #field_idx: <#field_type as ::hegel::generators::DefaultGenerator>::default_generator().boxed()
                    }
                })
                .collect();

            let default_bounds = default_gen_bounds(&field_types, quote! { 'a });

            let builder_methods: Vec<_> = field_indices
                .iter()
                .zip(field_types.iter())
                .map(|(field_idx, field_type)| {
                    quote! {
                        /// Set a custom generator for this field.
                        pub fn #field_idx<G>(mut self, generator: G) -> Self
                        where
                            G: ::hegel::generators::Generator<#field_type> + Send + Sync + 'a,
                        {
                            self.#field_idx = generator.boxed();
                            self
                        }
                    }
                })
                .collect();

            let field_generates: Vec<_> = field_indices
                .iter()
                .map(|field_idx| {
                    quote! { self.#field_idx.do_draw(__tc) }
                })
                .collect();

            quote! {
                #[doc = #struct_doc]
                pub struct #variant_generator_name<'a> {
                    #(#generator_fields,)*
                }

                impl<'a> #variant_generator_name<'a> {
                    /// Create a new generator with default generators for all fields.
                    pub fn new() -> Self
                    where
                        #(#default_bounds,)*
                    {
                        Self {
                            #(#new_inits,)*
                        }
                    }

                    #(#builder_methods)*
                }

                impl<'a> Default for #variant_generator_name<'a>
                where
                    #(#default_bounds,)*
                {
                    fn default() -> Self {
                        Self::new()
                    }
                }

                impl<'a> ::hegel::generators::Generator<#enum_name> for #variant_generator_name<'a> {
                    fn do_draw(&self, __tc: &::hegel::TestCase) -> #enum_name {
                        #enum_name::#variant_name(#(#field_generates,)*)
                    }
                }
            }
        }
    }
}
