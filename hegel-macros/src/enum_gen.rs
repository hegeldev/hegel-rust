use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{DeriveInput, Fields, Variant};

use crate::utils::{cbor_map, cbor_text, cbor_to_iter, default_gen_bounds, tuple_schema};

enum VariantKind<'a> {
    /// Unit variant like `Pending`
    Unit,
    /// Named fields like `Active { since: String }`
    Named {
        field_names: Vec<&'a syn::Ident>,
        field_types: Vec<&'a syn::Type>,
    },
    /// Single tuple field like `Write(String)`
    TupleSingle { field_type: &'a syn::Type },
    /// Multiple tuple fields like `Point(i32, i32)`
    TupleMultiple { field_types: Vec<&'a syn::Type> },
}

/// Classify a variant by its field structure.
fn classify_variant(variant: &Variant) -> VariantKind<'_> {
    match &variant.fields {
        Fields::Unit => VariantKind::Unit,
        Fields::Named(fields) => {
            let field_names: Vec<_> = fields
                .named
                .iter()
                .map(|f| f.ident.as_ref().unwrap())
                .collect();
            let field_types: Vec<_> = fields.named.iter().map(|f| &f.ty).collect();
            VariantKind::Named {
                field_names,
                field_types,
            }
        }
        Fields::Unnamed(fields) => {
            let field_types: Vec<_> = fields.unnamed.iter().map(|f| &f.ty).collect();
            if field_types.len() == 1 {
                VariantKind::TupleSingle {
                    field_type: field_types[0],
                }
            } else {
                VariantKind::TupleMultiple { field_types }
            }
        }
    }
}

/// Extract all field types from a variant.
fn variant_field_types(variant: &Variant) -> Vec<&syn::Type> {
    match classify_variant(variant) {
        VariantKind::Named { field_types, .. } | VariantKind::TupleMultiple { field_types } => {
            field_types
        }
        VariantKind::TupleSingle { field_type } => vec![field_type],
        VariantKind::Unit => vec![],
    }
}

/// Derive Generator for an enum.
pub(crate) fn derive_enum_generator(input: &DeriveInput, data: &syn::DataEnum) -> TokenStream {
    let enum_name = &input.ident;
    let generator_name = format_ident!("{}Generator", enum_name);

    // Collect variant information
    let variants: Vec<_> = data.variants.iter().collect();

    // Separate data variants from unit variants
    let data_variants: Vec<_> = variants
        .iter()
        .filter(|v| !matches!(classify_variant(v), VariantKind::Unit))
        .collect();

    // Generate variant generator structs for data variants
    let variant_generators: Vec<_> = data_variants
        .iter()
        .map(|variant| generate_variant_generator(enum_name, variant))
        .collect();

    // Generate field definitions for the main generator struct
    // Using PascalCase field names to match variant names
    let generator_fields: Vec<_> = data_variants
        .iter()
        .map(|variant| {
            let variant_name = &variant.ident;
            quote! {
                pub #variant_name: hegel::generators::BoxedGenerator<'a, #enum_name>
            }
        })
        .collect();

    // Generate default_VariantName() methods (take &self so they're accessible via default())
    let default_methods: Vec<_> = data_variants
        .iter()
        .map(|variant| {
            let variant_name = &variant.ident;
            let variant_generator_name = format_ident!("{}{}Generator", enum_name, variant_name);
            let default_method_name = format_ident!("default_{}", variant_name);

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
        .map(|variant| {
            let variant_name = &variant.ident;
            let variant_generator_name = format_ident!("{}{}Generator", enum_name, variant_name);

            quote! {
                #variant_name: #variant_generator_name::new().boxed()
            }
        })
        .collect();

    // Generator DefaultGenerate bounds for new()
    let default_bounds: Vec<_> = data_variants
        .iter()
        .flat_map(|variant| default_gen_bounds(&variant_field_types(variant), quote! { 'a }))
        .collect();

    // Generate VariantName() builder methods
    let with_methods: Vec<_> = data_variants
        .iter()
        .map(|variant| {
            let variant_name = &variant.ident;

            quote! {
                /// Set a custom generator for the #variant_name variant.
                #[allow(non_snake_case)]
                pub fn #variant_name<G>(mut self, generator: G) -> Self
                where
                    G: hegel::generators::Generator<#enum_name> + Send + Sync + 'a,
                {
                    self.#variant_name = generator.boxed();
                    self
                }
            }
        })
        .collect();

    // Build a `0..=N-1` integer schema for variant selection. This mirrors how
    // `gs::sampled_from` lowers to an integer-index schema
    let max_variant_idx = variants.len() - 1;
    let variant_index_schema = cbor_map(vec![
        (cbor_text("type"), cbor_text("integer")),
        (
            cbor_text("min_value"),
            quote! { hegel::ciborium::Value::Integer(hegel::ciborium::value::Integer::from(0u64)) },
        ),
        (
            cbor_text("max_value"),
            quote! { hegel::ciborium::Value::Integer(hegel::ciborium::value::Integer::from(#max_variant_idx as u64)) },
        ),
    ]);

    // Generate match arms for generate() compositional fallback, keyed by
    // declaration index.
    let generate_match_arms: Vec<_> = variants
        .iter()
        .enumerate()
        .map(|(i, variant)| {
            let variant_name = &variant.ident;
            match classify_variant(variant) {
                VariantKind::Unit => quote! { #i => #enum_name::#variant_name },
                _ => quote! { #i => self.#variant_name.do_draw(__tc) },
            }
        })
        .collect();

    // Handle case where there are no data variants (all unit)
    let generator_struct = if data_variants.is_empty() {
        quote! {
            /// Generated generator for #enum_name.
            pub struct #generator_name;

            impl #generator_name {
                /// Create a new generator.
                pub fn new() -> Self {
                    Self
                }
            }

            impl Default for #generator_name {
                fn default() -> Self {
                    Self::new()
                }
            }
        }
    } else {
        quote! {
            /// Generated generator for #enum_name.
            #[allow(non_snake_case)]
            pub struct #generator_name<'a> {
                #(#generator_fields,)*
            }

            #[allow(non_snake_case)]
            impl<'a> #generator_name<'a> {
                /// Create a new generator with default generators for all variants.
                pub fn new() -> Self
                where
                    #(#default_bounds,)*
                {
                    Self {
                        #(#new_field_inits,)*
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
        }
    };

    // Unit variant match arms keyed by declaration index for the all-unit
    // `as_basic` parse closure. Equivalent to `generate_match_arms` here
    // (every variant is unit) but kept separate for clarity.
    let unit_variant_match_arms: Vec<proc_macro2::TokenStream> = variants
        .iter()
        .enumerate()
        .map(|(i, variant)| {
            let variant_name = &variant.ident;
            quote! { #i => #enum_name::#variant_name }
        })
        .collect();

    let generate_trait_impl = if data_variants.is_empty() {
        // All-unit enum: pick a variant by integer index.
        quote! {
            impl hegel::generators::Generator<#enum_name> for #generator_name {
                fn do_draw(&self, __tc: &hegel::TestCase) -> #enum_name {
                    let basic = self.as_basic().unwrap();
                    basic.parse_raw(hegel::generate_raw(__tc, basic.schema()))
                }

                fn as_basic(&self) -> Option<hegel::generators::BasicGenerator<'_, #enum_name>> {
                    let schema = #variant_index_schema;
                    Some(hegel::generators::BasicGenerator::new(schema, |raw| {
                        let index: usize = hegel::generators::deserialize_value(raw);
                        match index {
                            #(#unit_variant_match_arms,)*
                            _ => unreachable!("Unknown variant index: {}", index),
                        }
                    }))
                }
            }
        }
    } else {
        // Mixed enum: try schema-based, fall back to compositional.
        //
        // Schema is `{"type": "one_of", "generators": [s_0, s_1, ...]}` with one
        // child per variant in declaration order. The server's response is
        // `[index, value]` where `index` selects the variant; for unit variants
        // the corresponding child schema is `{"type": "constant", "value": null}`
        // and the value is discarded, for data variants it's the variant
        // generator's schema.

        // Bind data variant basic generators (must succeed for all data variants).
        let data_variant_basic_bindings: Vec<proc_macro2::TokenStream> = data_variants
            .iter()
            .map(|variant| {
                let variant_name = &variant.ident;
                let basic_name = format_ident!("basic_{}", variant_name);
                quote! {
                    let #basic_name = self.#variant_name.as_basic()?;
                }
            })
            .collect();

        // Build schema entries and parse arms in declaration order so the wire
        // index matches the variant order.
        let null_schema = cbor_map(vec![
            (cbor_text("type"), cbor_text("constant")),
            (cbor_text("value"), quote! { hegel::ciborium::Value::Null }),
        ]);
        let one_of_schema_entries: Vec<proc_macro2::TokenStream> = variants
            .iter()
            .map(|variant| match classify_variant(variant) {
                VariantKind::Unit => null_schema.clone(),
                _ => {
                    let basic_name = format_ident!("basic_{}", variant.ident);
                    quote! { #basic_name.schema().clone() }
                }
            })
            .collect();

        let parse_arms: Vec<proc_macro2::TokenStream> = variants
            .iter()
            .enumerate()
            .map(|(i, variant)| {
                let variant_name = &variant.ident;
                match classify_variant(variant) {
                    VariantKind::Unit => quote! { #i => #enum_name::#variant_name },
                    _ => {
                        let basic_name = format_ident!("basic_{}", variant_name);
                        quote! { #i => #basic_name.parse_raw(value) }
                    }
                }
            })
            .collect();

        quote! {
            impl<'a> hegel::generators::Generator<#enum_name> for #generator_name<'a> {
                fn do_draw(&self, __tc: &hegel::TestCase) -> #enum_name {
                    if let Some(basic) = self.as_basic() {
                        basic.parse_raw(hegel::generate_raw(__tc, basic.schema()))
                    } else {
                        __tc.start_span(hegel::generators::labels::ENUM_VARIANT);
                        let index: usize = hegel::generate_from_schema(__tc,
                            &#variant_index_schema
                        );

                        let __result = match index {
                            #(#generate_match_arms,)*
                            _ => unreachable!("Unknown variant index: {}", index),
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
        }
    };

    let default_generator_impl = if data_variants.is_empty() {
        // All-unit enum: no lifetime on generator, no bounds needed
        quote! {
            impl hegel::generators::DefaultGenerator for #enum_name {
                type Generator = #generator_name;
                fn default_generator() -> Self::Generator {
                    #generator_name::new()
                }
            }
        }
    } else {
        // Mixed enum: generator has lifetime, needs DefaultGenerate bounds
        let default_generator_bounds: Vec<_> = data_variants
            .iter()
            .flat_map(|variant| {
                default_gen_bounds(&variant_field_types(variant), quote! { 'static })
            })
            .collect();

        quote! {
            impl hegel::generators::DefaultGenerator for #enum_name
            where
                #(#default_generator_bounds,)*
            {
                type Generator = #generator_name<'static>;
                fn default_generator() -> Self::Generator {
                    #generator_name::new()
                }
            }
        }
    };

    let expanded = quote! {
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

    match classify_variant(variant) {
        VariantKind::Unit => {
            // Unit variants don't get their own generator
            quote! {}
        }
        VariantKind::Named {
            field_names,
            field_types,
        } => {
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
        VariantKind::TupleSingle { field_type } => {
            quote! {
                /// Generated generator for the #variant_name variant of #enum_name.
                pub struct #variant_generator_name<'a> {
                    value: hegel::generators::BoxedGenerator<'a, #field_type>,
                }

                impl<'a> #variant_generator_name<'a> {
                    /// Create a new generator with the default generator for the field.
                    pub fn new() -> Self
                    where
                        #field_type: hegel::generators::DefaultGenerator,
                        <#field_type as hegel::generators::DefaultGenerator>::Generator: Send + Sync + 'a,
                    {
                        Self {
                            value: <#field_type as hegel::generators::DefaultGenerator>::default_generator().boxed(),
                        }
                    }

                    /// Set a custom generator for the value.
                    pub fn value<G>(mut self, generator: G) -> Self
                    where
                        G: hegel::generators::Generator<#field_type> + Send + Sync + 'a,
                    {
                        self.value = generator.boxed();
                        self
                    }
                }

                impl<'a> Default for #variant_generator_name<'a>
                where
                    #field_type: hegel::generators::DefaultGenerator,
                    <#field_type as hegel::generators::DefaultGenerator>::Generator: Send + Sync + 'a,
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
                            #enum_name::#variant_name(self.value.do_draw(__tc))
                        }
                    }

                    fn as_basic(&self) -> Option<hegel::generators::BasicGenerator<'_, #enum_name>> {
                        let value_basic = self.value.as_basic()?;
                        let schema = value_basic.schema().clone();

                        Some(hegel::generators::BasicGenerator::new(schema, move |raw| {
                            #enum_name::#variant_name(value_basic.parse_raw(raw))
                        }))
                    }
                }
            }
        }
        VariantKind::TupleMultiple { field_types } => {
            // Generate field names _0, _1, _2, etc.
            let field_indices: Vec<_> = (0..field_types.len())
                .map(|i| format_ident!("_{}", i))
                .collect();

            let builder_methods: Vec<_> = field_indices
                .iter()
                .zip(field_types.iter())
                .map(|(field_idx, field_type)| {
                    quote! {
                        /// Set a custom generator for this field.
                        pub fn #field_idx<G>(mut self, generator: G) -> Self
                        where
                            G: hegel::generators::Generator<#field_type> + Send + Sync + 'a,
                        {
                            self.#field_idx = generator.boxed();
                            self
                        }
                    }
                })
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
