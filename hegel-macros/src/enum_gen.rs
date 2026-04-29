use std::collections::HashMap;

use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{DeriveInput, Fields, Variant};

use crate::utils::{
    cbor_array, cbor_map, cbor_text, cbor_to_iter, default_gen_bounds, pascal_to_snake,
    tuple_schema,
};

/// Extract all field types from a variant.
fn variant_field_types(variant: &Variant) -> Vec<&syn::Type> {
    variant.fields.iter().map(|f| &f.ty).collect()
}

/// Derive Generator for an enum.
pub(crate) fn derive_enum_generator(input: &DeriveInput, data: &syn::DataEnum) -> TokenStream {
    let enum_name = &input.ident;
    let generator_name = format_ident!("{}Generator", enum_name);

    let variants: Vec<_> = data.variants.iter().collect();
    let data_variants: Vec<_> = variants
        .iter()
        .filter(|v| !matches!(v.fields, Fields::Unit))
        .collect();

    // Compute snake_case field names for each data variant.
    // If two variants would produce the same snake_case name, keep their original casing.
    let field_names: Vec<syn::Ident> = {
        let snake_names: Vec<String> = data_variants
            .iter()
            .map(|v| pascal_to_snake(&v.ident.to_string()))
            .collect();
        let mut counts: HashMap<&str, usize> = HashMap::new();
        for name in &snake_names {
            *counts.entry(name.as_str()).or_insert(0) += 1;
        }
        data_variants
            .iter()
            .zip(snake_names.iter())
            .map(|(v, snake)| {
                if counts[snake.as_str()] > 1 {
                    v.ident.clone()
                } else {
                    format_ident!("{}", snake)
                }
            })
            .collect()
    };

    // Generate variant generator structs for data variants
    let variant_generators: Vec<_> = data_variants
        .iter()
        .map(|variant| generate_variant_generator(enum_name, variant))
        .collect();

    // Generate field definitions for the main generator struct
    let generator_fields: Vec<_> = field_names
        .iter()
        .map(|field_name| {
            quote! {
                pub #field_name: hegel::generators::BoxedGenerator<'a, #enum_name>
            }
        })
        .collect();

    // Generate default_field_name() methods for named variants only
    let default_methods: Vec<_> = data_variants
        .iter()
        .zip(field_names.iter())
        .filter(|(v, _)| matches!(v.fields, Fields::Named(_)))
        .map(|(variant, field_name)| {
            let variant_name = &variant.ident;
            let variant_generator_name = format_ident!("{}{}Generator", enum_name, variant_name);
            let default_method_name = format_ident!("default_{}", field_name);

            let bounds = default_gen_bounds(&variant_field_types(variant), quote! { 'a });

            quote! {
                /// Get the default generator for the #variant_name variant.
                pub fn #default_method_name(&self) -> #variant_generator_name<'a>
                where
                    #(#bounds,)*
                {
                    #variant_generator_name::new()
                }
            }
        })
        .collect();

    // Generate new() field initializations (call variant generator directly)
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

    // Generator DefaultGenerate bounds for new()
    let default_bounds: Vec<_> = data_variants
        .iter()
        .flat_map(|variant| default_gen_bounds(&variant_field_types(variant), quote! { 'a }))
        .collect();

    // builder methods
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

                    quote! {
                        /// Set a custom generator for the #variant_name variant.
                        pub fn #field_name<F>(mut self, configure: F) -> Self
                        where
                            F: FnOnce(#variant_generator_name<'a>) -> #variant_generator_name<'a>,
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
                                #gtp: hegel::generators::Generator<#ft> + Send + Sync + 'a
                            }
                        })
                        .collect();

                    quote! {
                        /// Set custom generators for the #variant_name variant.
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
                    }
                }
            }
        })
        .collect();

    let variant_names: Vec<_> = variants.iter().map(|v| v.ident.to_string()).collect();
    let variant_names_schema = {
        let values: Vec<_> = variant_names.iter().map(|name| cbor_text(name)).collect();
        cbor_map(vec![
            (cbor_text("type"), cbor_text("sampled_from")),
            (cbor_text("values"), cbor_array(values)),
        ])
    };

    // Map from data variant name to its field ident
    let variant_to_field: HashMap<String, &syn::Ident> = data_variants
        .iter()
        .zip(field_names.iter())
        .map(|(v, f)| (v.ident.to_string(), f))
        .collect();

    // Generate match arms for generate() compositional fallback
    let generate_match_arms: Vec<_> = variants
        .iter()
        .map(|variant| {
            let variant_name = &variant.ident;
            let variant_name_str = variant.ident.to_string();

            match &variant.fields {
                Fields::Unit => {
                    quote! {
                        #variant_name_str => #enum_name::#variant_name
                    }
                }
                _ => {
                    let field_name = variant_to_field[&variant_name_str];
                    quote! {
                        #variant_name_str => self.#field_name.do_draw(__tc)
                    }
                }
            }
        })
        .collect();

    let generator_struct = quote! {
        /// Generated generator for #enum_name.
        pub struct #generator_name<'a> {
            #(#generator_fields,)*
            _phantom: std::marker::PhantomData<&'a ()>,
        }

        impl<'a> #generator_name<'a> {
            /// Create a new generator with default generators for all variants.
            pub fn new() -> Self
            where
                #(#default_bounds,)*
            {
                Self {
                    #(#new_field_inits,)*
                    _phantom: std::marker::PhantomData,
                }
            }

            #(#default_methods)*

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

    // Schema is `{"type": "one_of", "generators": [s_0, s_1, ...]}` with one
    // child per variant in declaration order. The server's response is
    // `[index, value]` where `index` selects the variant; for unit variants
    // the corresponding child schema is `{"type": "null"}` and the value is
    // discarded, for data variants it's the variant generator's schema.

    // Bind data variant basic generators (must succeed for all data variants).
    let data_variant_basic_bindings: Vec<proc_macro2::TokenStream> = field_names
        .iter()
        .map(|field_name| {
            let basic_name = format_ident!("basic_{}", field_name);
            quote! {
                let #basic_name = self.#field_name.as_basic()?;
            }
        })
        .collect();

    // Build schema entries and parse arms in declaration order so the wire
    // index matches the variant order.
    let null_schema = cbor_map(vec![(cbor_text("type"), cbor_text("null"))]);
    let one_of_schema_entries: Vec<proc_macro2::TokenStream> = variants
        .iter()
        .map(|variant| match &variant.fields {
            Fields::Unit => null_schema.clone(),
            _ => {
                let variant_name_str = variant.ident.to_string();
                let field_name = variant_to_field[&variant_name_str];
                let basic_name = format_ident!("basic_{}", field_name);
                quote! { #basic_name.schema().clone() }
            }
        })
        .collect();

    let parse_arms: Vec<proc_macro2::TokenStream> = variants
        .iter()
        .enumerate()
        .map(|(i, variant)| {
            let variant_name = &variant.ident;
            match &variant.fields {
                Fields::Unit => quote! { #i => #enum_name::#variant_name },
                _ => {
                    let variant_name_str = variant.ident.to_string();
                    let field_name = variant_to_field[&variant_name_str];
                    let basic_name = format_ident!("basic_{}", field_name);
                    quote! { #i => #basic_name.parse_raw(value) }
                }
            }
        })
        .collect();

    let generate_trait_impl = quote! {
        impl<'a> hegel::generators::Generator<#enum_name> for #generator_name<'a> {
            fn do_draw(&self, __tc: &hegel::TestCase) -> #enum_name {
                if let Some(basic) = self.as_basic() {
                    basic.parse_raw(hegel::generate_raw(__tc, basic.schema()))
                } else {
                    __tc.start_span(hegel::generators::labels::ENUM_VARIANT);
                    let selected: String = hegel::generate_from_schema(__tc,
                        &#variant_names_schema
                    );

                    let __result = match selected.as_str() {
                        #(#generate_match_arms,)*
                        _ => unreachable!("Unknown variant: {}", selected),
                    };
                    __tc.stop_span(false);
                    __result
                }
            }

            fn as_basic(&self) -> Option<hegel::generators::BasicGenerator<'_, #enum_name>> {
                #(#data_variant_basic_bindings)*

                let one_of_schemas: Vec<hegel::ciborium::Value> = vec![
                    #(#one_of_schema_entries,)*
                ];

                let schema = hegel::ciborium::Value::Map(vec![
                    (
                        hegel::ciborium::Value::Text("type".to_string()),
                        hegel::ciborium::Value::Text("one_of".to_string()),
                    ),
                    (
                        hegel::ciborium::Value::Text("generators".to_string()),
                        hegel::ciborium::Value::Array(one_of_schemas),
                    ),
                ]);

                Some(hegel::generators::BasicGenerator::new(schema, move |raw| {
                    // The server returns `[index, value]` for one_of schemas.
                    let [idx, value]: [hegel::ciborium::Value; 2] =
                        raw.into_array().unwrap().try_into().unwrap();
                    let index = i128::from(idx.into_integer().unwrap()) as usize;
                    match index {
                        #(#parse_arms,)*
                        _ => panic!("Unknown variant index: {}", index),
                    }
                }))
            }
        }
    };

    let default_generator_bounds: Vec<_> = data_variants
        .iter()
        .flat_map(|variant| default_gen_bounds(&variant_field_types(variant), quote! { 'static }))
        .collect();

    let default_generator_impl = quote! {
        impl hegel::generators::DefaultGenerator for #enum_name
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
        // if a user has non-camel-case types that conflict, we will generate warning-emitting variable and type
        // names here. We want to suppress these warnings, because the user already had to suppress these same warnings
        // when they constructed their type, and they have no way to reach down into this block to locally-supress them
        // and would have to suppress on their entire module, which is onerous.
        #[allow(non_camel_case_types, non_snake_case)]
        const _: () = {
            use hegel::generators::Generator as _;

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

    match &variant.fields {
        Fields::Unit => {
            // Unit variants don't get their own generator
            quote! {}
        }
        Fields::Named(fields) => {
            let field_names: Vec<_> = fields
                .named
                .iter()
                .map(|f| f.ident.as_ref().unwrap())
                .collect();
            let field_types: Vec<_> = fields.named.iter().map(|f| &f.ty).collect();
            // Generate field builder methods (same name as field, no prefix)
            let builder_methods: Vec<_> = field_names
                .iter()
                .zip(field_types.iter())
                .map(|(field_name, field_type)| {
                    quote! {
                        /// Set a custom generator for this field.
                        pub fn #field_name<G>(mut self, generator: G) -> Self
                        where
                            G: hegel::generators::Generator<#field_type> + Send + Sync + 'a,
                        {
                            self.#field_name = generator.boxed();
                            self
                        }
                    }
                })
                .collect();

            // Generate field definitions
            let generator_fields: Vec<_> = field_names
                .iter()
                .zip(field_types.iter())
                .map(|(field_name, field_type)| {
                    quote! { #field_name: hegel::generators::BoxedGenerator<'a, #field_type> }
                })
                .collect();

            // Generate new() initializers
            let new_inits: Vec<_> = field_names
                .iter()
                .zip(field_types.iter())
                .map(|(field_name, field_type)| {
                    quote! {
                        #field_name: <#field_type as hegel::generators::DefaultGenerator>::default_generator().boxed()
                    }
                })
                .collect();

            // Generator Default bounds
            let default_bounds = default_gen_bounds(&field_types, quote! { 'a });

            // Generate field construction in generate()
            let field_constructions: Vec<_> = field_names
                .iter()
                .map(|field_name| {
                    quote! { #field_name: self.#field_name.do_draw(__tc) }
                })
                .collect();

            // Basic bindings
            let basic_bindings: Vec<proc_macro2::TokenStream> = field_names
                .iter()
                .map(|name| {
                    let basic_name = format_ident!("basic_{}", name);
                    quote! { let #basic_name = self.#name.as_basic()?; }
                })
                .collect();

            // Schema elements (positional, in field order)
            let schema_elements: Vec<_> = field_names
                .iter()
                .map(|name| {
                    let basic_name = format_ident!("basic_{}", name);
                    quote! { #basic_name.schema().clone() }
                })
                .collect();

            let schema_ts = tuple_schema(schema_elements);
            let parse_iter_ts =
                cbor_to_iter("iter", quote! { raw }, "Expected tuple for variant fields");

            // parse closure field extractions (positional from tuple)
            let field_parse_in_closure: Vec<proc_macro2::TokenStream> = field_names
                .iter()
                .map(|name| {
                    let basic_name = format_ident!("basic_{}", name);
                    quote! {
                        let #name = #basic_name.parse_raw(
                            iter.next().unwrap_or_else(|| panic!("Missing element in tuple"))
                        );
                    }
                })
                .collect();

            quote! {
                /// Generated generator for the #variant_name variant of #enum_name.
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

                impl<'a> hegel::generators::Generator<#enum_name> for #variant_generator_name<'a> {
                    fn do_draw(&self, __tc: &hegel::TestCase) -> #enum_name {

                        if let Some(basic) = self.as_basic() {
                            basic.parse_raw(hegel::generate_raw(__tc, basic.schema()))
                        } else {
                            #enum_name::#variant_name {
                                #(#field_constructions,)*
                            }
                        }
                    }

                    fn as_basic(&self) -> Option<hegel::generators::BasicGenerator<'_, #enum_name>> {
                        #(#basic_bindings)*

                        let schema = #schema_ts;

                        Some(hegel::generators::BasicGenerator::new(schema, move |raw| {
                            #parse_iter_ts

                            #(#field_parse_in_closure)*

                            #enum_name::#variant_name {
                                #(#field_names,)*
                            }
                        }))
                    }
                }
            }
        }
        Fields::Unnamed(fields) => {
            let field_types: Vec<_> = fields.unnamed.iter().map(|f| &f.ty).collect();
            // Generate field names _0, _1, _2, etc.
            let field_indices: Vec<_> = (0..field_types.len())
                .map(|i| format_ident!("_{}", i))
                .collect();

            let generator_fields: Vec<_> = field_indices
                .iter()
                .zip(field_types.iter())
                .map(|(field_idx, field_type)| {
                    quote! { #field_idx: hegel::generators::BoxedGenerator<'a, #field_type> }
                })
                .collect();

            let new_inits: Vec<_> = field_indices
                .iter()
                .zip(field_types.iter())
                .map(|(field_idx, field_type)| {
                    quote! {
                        #field_idx: <#field_type as hegel::generators::DefaultGenerator>::default_generator().boxed()
                    }
                })
                .collect();

            let default_bounds = default_gen_bounds(&field_types, quote! { 'a });

            let field_generates: Vec<_> = field_indices
                .iter()
                .map(|field_idx| {
                    quote! { self.#field_idx.do_draw(__tc) }
                })
                .collect();

            // Basic bindings for tuple fields
            let basic_bindings: Vec<proc_macro2::TokenStream> = field_indices
                .iter()
                .map(|idx| {
                    let basic_name = format_ident!("basic{}", idx);
                    quote! { let #basic_name = self.#idx.as_basic()?; }
                })
                .collect();

            let schema_elements: Vec<proc_macro2::TokenStream> = field_indices
                .iter()
                .map(|idx| {
                    let basic_name = format_ident!("basic{}", idx);
                    quote! { #basic_name.schema().clone() }
                })
                .collect();

            // parse closure extractions
            let parse_raw_extractions: Vec<proc_macro2::TokenStream> = field_indices
                .iter()
                .map(|idx| {
                    let basic_name = format_ident!("basic{}", idx);
                    quote! {
                        let #idx = #basic_name.parse_raw(
                            iter.next().unwrap_or_else(|| panic!("Tuple variant missing element"))
                        );
                    }
                })
                .collect();

            let schema_ts = tuple_schema(schema_elements);
            let parse_iter_ts =
                cbor_to_iter("iter", quote! { raw }, "Expected tuple for variant fields");

            quote! {
                /// Generated generator for the #variant_name variant of #enum_name.
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
                }

                impl<'a> Default for #variant_generator_name<'a>
                where
                    #(#default_bounds,)*
                {
                    fn default() -> Self {
                        Self::new()
                    }
                }

                impl<'a> hegel::generators::Generator<#enum_name> for #variant_generator_name<'a> {
                    fn do_draw(&self, __tc: &hegel::TestCase) -> #enum_name {

                        if let Some(basic) = self.as_basic() {
                            basic.parse_raw(hegel::generate_raw(__tc, basic.schema()))
                        } else {
                            #enum_name::#variant_name(#(#field_generates,)*)
                        }
                    }

                    fn as_basic(&self) -> Option<hegel::generators::BasicGenerator<'_, #enum_name>> {
                        #(#basic_bindings)*

                        let schema = #schema_ts;

                        Some(hegel::generators::BasicGenerator::new(schema, move |raw| {
                            #parse_iter_ts

                            #(#parse_raw_extractions)*

                            #enum_name::#variant_name(#(#field_indices,)*)
                        }))
                    }
                }
            }
        }
    }
}
