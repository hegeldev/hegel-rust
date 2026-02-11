//! Derive macros for the Hegel property-based testing SDK.
//!
//! This crate provides `#[derive(Generate)]` for automatic generator derivation.

use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{parse_macro_input, Data, DeriveInput, Fields, Variant};

/// Derive a generator for a struct or enum.
///
/// For structs, this creates a `<StructName>Generator` type with:
/// - `new()` - creates a generator using default generators for all fields
/// - `with_<field>(gen)` - builder method to customize each field's generator
///
/// For enums, this creates:
/// - `<EnumName><VariantName>Generator` for each variant with data
/// - `<EnumName>Generator` with public fields for each data variant's generator
/// - `default_<VariantName>()` factory methods for default variant generators
/// - `with_<VariantName>(gen)` builder methods to customize variant generation
///
/// # Struct Example
///
/// ```ignore
/// use hegel::Generate;
///
/// #[derive(Generate)]
/// struct Person {
///     name: String,
///     age: u32,
/// }
///
/// // Creates PersonGenerator with:
/// // - PersonGenerator::new()
/// // - .with_name(gen)
/// // - .with_age(gen)
///
/// let gen = PersonGenerator::new()
///     .with_age(hegel::gen::integers::<u32>().with_min(0).with_max(120));
///
/// let person: Person = gen.generate();
/// ```
///
/// # Enum Example
///
/// ```ignore
/// use hegel::Generate;
///
/// #[derive(Generate)]
/// enum Status {
///     Pending,
///     Active { since: String },
///     Error { code: i32, message: String },
/// }
///
/// // Creates:
/// // - StatusActiveGenerator with .with_since(gen)
/// // - StatusErrorGenerator with .with_code(gen), .with_message(gen)
/// // - StatusGenerator with .Active, .Error fields and .with_Active(gen), etc.
///
/// let gen = StatusGenerator::new()
///     .with_Active(
///         StatusGenerator::default_Active()
///             .with_since(hegel::gen::text().with_max_size(20))
///     );
///
/// let status: Status = gen.generate();
/// ```
#[proc_macro_derive(Generate)]
pub fn derive_generate(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    match &input.data {
        Data::Struct(data) => derive_struct_generate(&input, data),
        Data::Enum(data) => derive_enum_generate(&input, data),
        Data::Union(_) => syn::Error::new_spanned(&input, "Generate cannot be derived for unions")
            .to_compile_error()
            .into(),
    }
}

/// Helper to generate a cbor map expression in proc macro output.
/// Returns a token stream that creates `ciborium::Value::Map(vec![...])`.
fn cbor_map_tokens(
    entries: Vec<(String, proc_macro2::TokenStream)>,
) -> proc_macro2::TokenStream {
    let pairs: Vec<proc_macro2::TokenStream> = entries
        .into_iter()
        .map(|(key, value)| {
            quote! {
                (
                    ciborium::Value::Text(#key.to_string()),
                    #value,
                )
            }
        })
        .collect();

    quote! {
        ciborium::Value::Map(vec![#(#pairs),*])
    }
}

/// Derive Generate for a struct.
fn derive_struct_generate(input: &DeriveInput, data: &syn::DataStruct) -> TokenStream {
    let name = &input.ident;
    let generator_name = format_ident!("{}Generator", name);

    let fields = match &data.fields {
        Fields::Named(fields) => &fields.named,
        Fields::Unnamed(_) => {
            return syn::Error::new_spanned(
                input,
                "Generate can only be derived for structs with named fields",
            )
            .to_compile_error()
            .into();
        }
        Fields::Unit => {
            return syn::Error::new_spanned(input, "Generate cannot be derived for unit structs")
                .to_compile_error()
                .into();
        }
    };

    let field_names: Vec<_> = fields
        .iter()
        .map(|f| f.ident.as_ref().unwrap())
        .collect();

    let field_types: Vec<_> = fields.iter().map(|f| &f.ty).collect();

    // Generate the with_* method names
    let with_methods: Vec<_> = field_names
        .iter()
        .map(|name| format_ident!("with_{}", name))
        .collect();

    // Generate field definitions for the generator struct
    let generator_fields = field_names.iter().zip(field_types.iter()).map(|(name, ty)| {
        quote! {
            #name: hegel::gen::BoxedGenerator<'a, #ty>
        }
    });

    // Generate the new() constructor
    let new_field_inits = field_types.iter().map(|ty| {
        quote! {
            <#ty as hegel::gen::DefaultGenerator>::default_generator().boxed()
        }
    });

    let new_fields = field_names.iter().zip(new_field_inits).map(|(name, init)| {
        quote! { #name: #init }
    });

    // Generate Default trait bounds for new()
    let default_bounds = field_types.iter().map(|ty| {
        quote! {
            #ty: hegel::gen::DefaultGenerator,
            <#ty as hegel::gen::DefaultGenerator>::Generator: Send + Sync + 'a
        }
    });

    // Generate with_* methods
    let with_method_impls = field_names.iter().zip(field_types.iter()).zip(with_methods.iter())
        .map(|((field_name, field_type), method_name)| {
            quote! {
                /// Set a custom generator for this field.
                pub fn #method_name<G>(mut self, gen: G) -> Self
                where
                    G: hegel::gen::Generate<#field_type> + Send + Sync + 'a,
                {
                    self.#field_name = gen.boxed();
                    self
                }
            }
        });

    // Generate the generate() fallback fields
    let generate_fields = field_names.iter().map(|name| {
        quote! {
            #name: self.#name.generate()
        }
    });

    // Generate field name strings for schema
    let field_name_strings: Vec<String> = field_names.iter().map(|n| n.to_string()).collect();

    // Generate per-field basic variable bindings: let b_field = self.field.as_basic()?;
    let basic_bindings: Vec<proc_macro2::TokenStream> = field_names
        .iter()
        .map(|name| {
            let basic_name = format_ident!("b_{}", name);
            quote! { let #basic_name = self.#name.as_basic()?; }
        })
        .collect();

    // Generate schema properties entries
    let schema_properties: Vec<proc_macro2::TokenStream> = field_names
        .iter()
        .zip(field_name_strings.iter())
        .map(|(name, name_str)| {
            let basic_name = format_ident!("b_{}", name);
            quote! {
                (
                    ciborium::Value::Text(#name_str.to_string()),
                    #basic_name.schema.clone(),
                )
            }
        })
        .collect();

    // Generate required entries
    let schema_required: Vec<proc_macro2::TokenStream> = field_name_strings
        .iter()
        .map(|name_str| {
            quote! { ciborium::Value::Text(#name_str.to_string()) }
        })
        .collect();

    // Generate transform captures: let t_field = b_field.transform;
    let transform_captures: Vec<proc_macro2::TokenStream> = field_names
        .iter()
        .map(|name| {
            let basic_name = format_ident!("b_{}", name);
            let transform_name = format_ident!("t_{}", name);
            quote! { let #transform_name = #basic_name.transform; }
        })
        .collect();

    // Generate per-field extraction + deserialization in the transform closure
    let field_extractions: Vec<proc_macro2::TokenStream> = field_names
        .iter()
        .zip(field_name_strings.iter())
        .map(|(name, name_str)| {
            let transform_name = format_ident!("t_{}", name);
            quote! {
                let #name = {
                    let raw_val = fields.remove(#name_str)
                        .unwrap_or_else(|| panic!("hegel: missing field '{}' in object", #name_str));
                    if let Some(ref t) = #transform_name {
                        t(raw_val)
                    } else {
                        hegel::gen::deserialize_value(raw_val)
                    }
                };
            }
        })
        .collect();

    let construct_fields: Vec<&syn::Ident> = field_names.clone();

    let expanded = quote! {
        /// Generated generator for #name.
        pub struct #generator_name<'a> {
            #(#generator_fields,)*
        }

        impl<'a> #generator_name<'a> {
            /// Create a new generator with default generators for all fields.
            pub fn new() -> Self
            where
                #(#default_bounds,)*
            {
                Self {
                    #(#new_fields,)*
                }
            }

            #(#with_method_impls)*
        }

        impl<'a> Default for #generator_name<'a>
        where
            #(#field_types: hegel::gen::DefaultGenerator,)*
            #(<#field_types as hegel::gen::DefaultGenerator>::Generator: Send + Sync + 'a,)*
        {
            fn default() -> Self {
                Self::new()
            }
        }

        impl<'a> hegel::gen::Generate<#name> for #generator_name<'a> {
            fn generate(&self) -> #name {
                use hegel::gen::Generate;
                if let Some(basic) = self.as_basic() {
                    basic.generate_transformed()
                } else {
                    hegel::gen::group(hegel::gen::labels::FIXED_DICT, || {
                        #name {
                            #(#generate_fields,)*
                        }
                    })
                }
            }

            fn as_basic(&self) -> Option<hegel::gen::BasicGenerator<#name>> {
                use hegel::gen::Generate;

                #(#basic_bindings)*

                let schema = ciborium::Value::Map(vec![
                    (
                        ciborium::Value::Text("type".to_string()),
                        ciborium::Value::Text("object".to_string()),
                    ),
                    (
                        ciborium::Value::Text("properties".to_string()),
                        ciborium::Value::Map(vec![
                            #(#schema_properties,)*
                        ]),
                    ),
                    (
                        ciborium::Value::Text("required".to_string()),
                        ciborium::Value::Array(vec![
                            #(#schema_required,)*
                        ]),
                    ),
                ]);

                #(#transform_captures)*

                Some(hegel::gen::BasicGenerator::with_transform(schema, move |raw| {
                    let mut fields: std::collections::HashMap<String, ciborium::Value> = match raw {
                        ciborium::Value::Map(entries) => {
                            entries.into_iter().filter_map(|(k, v)| {
                                if let ciborium::Value::Text(key) = k {
                                    Some((key, v))
                                } else {
                                    None
                                }
                            }).collect()
                        }
                        _ => panic!("hegel: expected object from struct schema, got {:?}", raw),
                    };

                    #(#field_extractions)*

                    #name {
                        #(#construct_fields,)*
                    }
                }))
            }
        }
    };

    TokenStream::from(expanded)
}

/// Classification of an enum variant for code generation.
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

/// Derive Generate for an enum.
fn derive_enum_generate(input: &DeriveInput, data: &syn::DataEnum) -> TokenStream {
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
                pub #variant_name: hegel::gen::BoxedGenerator<'a, #enum_name>
            }
        })
        .collect();

    // Generate default_VariantName() factory methods
    let default_methods: Vec<_> = data_variants
        .iter()
        .map(|variant| {
            let variant_name = &variant.ident;
            let variant_generator_name = format_ident!("{}{}Generator", enum_name, variant_name);
            let default_method_name = format_ident!("default_{}", variant_name);

            // Get the bounds for this variant
            let bounds: Vec<_> = match classify_variant(variant) {
                VariantKind::Named { field_types, .. } => field_types
                    .into_iter()
                    .map(|ty| {
                        quote! {
                            #ty: hegel::gen::DefaultGenerator,
                            <#ty as hegel::gen::DefaultGenerator>::Generator: Send + Sync + 'a
                        }
                    })
                    .collect(),
                VariantKind::TupleSingle { field_type } => vec![quote! {
                    #field_type: hegel::gen::DefaultGenerator,
                    <#field_type as hegel::gen::DefaultGenerator>::Generator: Send + Sync + 'a
                }],
                VariantKind::TupleMultiple { field_types } => field_types
                    .into_iter()
                    .map(|ty| {
                        quote! {
                            #ty: hegel::gen::DefaultGenerator,
                            <#ty as hegel::gen::DefaultGenerator>::Generator: Send + Sync + 'a
                        }
                    })
                    .collect(),
                VariantKind::Unit => vec![],
            };

            quote! {
                /// Get the default generator for the #variant_name variant.
                pub fn #default_method_name() -> #variant_generator_name<'a>
                where
                    #(#bounds,)*
                {
                    #variant_generator_name::new()
                }
            }
        })
        .collect();

    // Generate new() field initializations
    let new_field_inits: Vec<_> = data_variants
        .iter()
        .map(|variant| {
            let variant_name = &variant.ident;
            let default_method_name = format_ident!("default_{}", variant_name);

            quote! {
                #variant_name: Self::#default_method_name().boxed()
            }
        })
        .collect();

    // Generate DefaultGenerator bounds for new()
    let default_bounds: Vec<_> = data_variants
        .iter()
        .flat_map(|variant| {
            match classify_variant(variant) {
                VariantKind::Named { field_types, .. } => field_types
                    .into_iter()
                    .map(|ty| {
                        quote! {
                            #ty: hegel::gen::DefaultGenerator,
                            <#ty as hegel::gen::DefaultGenerator>::Generator: Send + Sync + 'a
                        }
                    })
                    .collect::<Vec<_>>(),
                VariantKind::TupleSingle { field_type } => vec![quote! {
                    #field_type: hegel::gen::DefaultGenerator,
                    <#field_type as hegel::gen::DefaultGenerator>::Generator: Send + Sync + 'a
                }],
                VariantKind::TupleMultiple { field_types } => field_types
                    .into_iter()
                    .map(|ty| {
                        quote! {
                            #ty: hegel::gen::DefaultGenerator,
                            <#ty as hegel::gen::DefaultGenerator>::Generator: Send + Sync + 'a
                        }
                    })
                    .collect::<Vec<_>>(),
                VariantKind::Unit => vec![],
            }
        })
        .collect();

    // Generate with_VariantName() builder methods
    let with_methods: Vec<_> = data_variants
        .iter()
        .map(|variant| {
            let variant_name = &variant.ident;
            let with_method_name = format_ident!("with_{}", variant_name);

            quote! {
                /// Set a custom generator for the #variant_name variant.
                pub fn #with_method_name<G>(mut self, gen: G) -> Self
                where
                    G: hegel::gen::Generate<#enum_name> + Send + Sync + 'a,
                {
                    self.#variant_name = gen.boxed();
                    self
                }
            }
        })
        .collect();

    // Generate variant names for generate
    let all_variant_names: Vec<_> = variants.iter().map(|v| v.ident.to_string()).collect();

    // Build sampled_from schema for variant selection
    let sampled_from_schema = {
        let variant_cbor_values: Vec<_> = all_variant_names.iter().map(|name| {
            quote! { ciborium::Value::Text(#name.to_string()) }
        }).collect();

        cbor_map_tokens(vec![
            ("sampled_from".to_string(), quote! { ciborium::Value::Array(vec![#(#variant_cbor_values),*]) }),
        ])
    };

    // Generate match arms for generate() compositional fallback
    let generate_match_arms: Vec<_> = variants
        .iter()
        .map(|variant| {
            let variant_name = &variant.ident;
            let variant_name_str = variant.ident.to_string();

            match classify_variant(variant) {
                VariantKind::Unit => {
                    quote! {
                        #variant_name_str => #enum_name::#variant_name
                    }
                }
                _ => {
                    quote! {
                        #variant_name_str => self.#variant_name.generate()
                    }
                }
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

    // Unit variant match arms for the transform closure (used by as_basic)
    let unit_variant_match_arms: Vec<proc_macro2::TokenStream> = variants
        .iter()
        .filter(|v| matches!(classify_variant(v), VariantKind::Unit))
        .map(|variant| {
            let variant_name = &variant.ident;
            let variant_name_str = variant.ident.to_string();
            quote! { #variant_name_str => #enum_name::#variant_name }
        })
        .collect();

    let data_variant_names: Vec<_> = data_variants.iter().map(|v| &v.ident).collect();
    let data_variant_name_strings: Vec<String> = data_variant_names.iter().map(|n| n.to_string()).collect();

    let generate_trait_impl = if data_variants.is_empty() {
        // All-unit enum: basic with sampled_from schema + transform
        quote! {
            impl hegel::gen::Generate<#enum_name> for #generator_name {
                fn generate(&self) -> #enum_name {
                    if let Some(basic) = self.as_basic() {
                        basic.generate_transformed()
                    } else {
                        unreachable!("unit-only enum is always basic")
                    }
                }

                fn as_basic(&self) -> Option<hegel::gen::BasicGenerator<#enum_name>> {
                    let schema = #sampled_from_schema;

                    Some(hegel::gen::BasicGenerator::with_transform(schema, |raw| {
                        let selected: String = hegel::gen::deserialize_value(raw);
                        match selected.as_str() {
                            #(#unit_variant_match_arms,)*
                            _ => unreachable!("Unknown variant: {}", selected),
                        }
                    }))
                }
            }
        }
    } else {
        // Mixed enum: try basic, fall back to compositional
        // Build one_of schema from unit const schemas + data variant schemas
        let unit_variant_const_schemas: Vec<proc_macro2::TokenStream> = variants
            .iter()
            .filter(|v| matches!(classify_variant(v), VariantKind::Unit))
            .map(|variant| {
                let variant_name_str = variant.ident.to_string();
                quote! {
                    ciborium::Value::Map(vec![
                        (
                            ciborium::Value::Text("const".to_string()),
                            ciborium::Value::Text(#variant_name_str.to_string()),
                        ),
                    ])
                }
            })
            .collect();

        quote! {
            impl<'a> hegel::gen::Generate<#enum_name> for #generator_name<'a> {
                fn generate(&self) -> #enum_name {
                    use hegel::gen::Generate;
                    if let Some(basic) = self.as_basic() {
                        basic.generate_transformed()
                    } else {
                        hegel::gen::group(hegel::gen::labels::ENUM_VARIANT, || {
                            let selected: String = hegel::gen::generate_from_schema(
                                &#sampled_from_schema
                            );

                            match selected.as_str() {
                                #(#generate_match_arms,)*
                                _ => unreachable!("Unknown variant: {}", selected),
                            }
                        })
                    }
                }

                fn as_basic(&self) -> Option<hegel::gen::BasicGenerator<#enum_name>> {
                    use hegel::gen::Generate;

                    // Try to get basic generators for all data variants
                    #(
                        let #data_variant_names = self.#data_variant_names.as_basic()?;
                    )*

                    // Build one_of schema: unit variants as const schemas, data variants from as_basic
                    let mut one_of_schemas: Vec<ciborium::Value> = vec![
                        #(#unit_variant_const_schemas,)*
                    ];

                    #(
                        one_of_schemas.push(#data_variant_names.schema.clone());
                    )*

                    let schema = ciborium::Value::Map(vec![
                        (
                            ciborium::Value::Text("one_of".to_string()),
                            ciborium::Value::Array(one_of_schemas),
                        ),
                    ]);

                    // Capture transforms from data variant generators
                    #(
                        let #data_variant_names = #data_variant_names.transform;
                    )*

                    let data_variant_names: Vec<String> = vec![#(#data_variant_name_strings.to_string()),*];

                    Some(hegel::gen::BasicGenerator::with_transform(schema, move |raw| {
                        match &raw {
                            // String -> unit variant
                            ciborium::Value::Text(s) => {
                                match s.as_str() {
                                    #(#unit_variant_match_arms,)*
                                    _ => panic!("hegel: unknown unit variant: {}", s),
                                }
                            }
                            // Map -> data variant (look at first key)
                            ciborium::Value::Map(entries) => {
                                let first_key = entries.first()
                                    .and_then(|(k, _)| if let ciborium::Value::Text(s) = k { Some(s.as_str()) } else { None })
                                    .unwrap_or_else(|| panic!("hegel: expected map with string key for data variant"));

                                // Match against data variant names and apply the right transform
                                #(
                                    if first_key == #data_variant_name_strings {
                                        if let Some(ref t) = #data_variant_names {
                                            return t(raw);
                                        } else {
                                            panic!("hegel: data variant transform missing for '{}'", first_key);
                                        }
                                    }
                                )*

                                panic!("hegel: unknown data variant key: {}", first_key);
                            }
                            _ => panic!("hegel: expected string or map for enum, got {:?}", raw),
                        }
                    }))
                }
            }
        }
    };

    let expanded = quote! {
        #(#variant_generators)*

        #generator_struct

        #generate_trait_impl
    };

    TokenStream::from(expanded)
}

/// Generate a variant generator struct for a data variant.
fn generate_variant_generator(enum_name: &syn::Ident, variant: &Variant) -> proc_macro2::TokenStream {
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
            // Generate with_field methods
            let with_methods: Vec<_> = field_names
                .iter()
                .zip(field_types.iter())
                .map(|(field_name, field_type)| {
                    let with_method_name = format_ident!("with_{}", field_name);
                    quote! {
                        /// Set a custom generator for this field.
                        pub fn #with_method_name<G>(mut self, gen: G) -> Self
                        where
                            G: hegel::gen::Generate<#field_type> + Send + Sync + 'a,
                        {
                            self.#field_name = gen.boxed();
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
                    quote! { #field_name: hegel::gen::BoxedGenerator<'a, #field_type> }
                })
                .collect();

            // Generate new() initializers
            let new_inits: Vec<_> = field_names
                .iter()
                .zip(field_types.iter())
                .map(|(field_name, field_type)| {
                    quote! {
                        #field_name: <#field_type as hegel::gen::DefaultGenerator>::default_generator().boxed()
                    }
                })
                .collect();

            // Generate Default bounds
            let default_bounds: Vec<_> = field_types
                .iter()
                .map(|ty| {
                    quote! {
                        #ty: hegel::gen::DefaultGenerator,
                        <#ty as hegel::gen::DefaultGenerator>::Generator: Send + Sync + 'a
                    }
                })
                .collect();

            // Generate field construction in generate()
            let field_constructions: Vec<_> = field_names
                .iter()
                .map(|field_name| {
                    quote! { #field_name: self.#field_name.generate() }
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

                    #(#with_methods)*
                }

                impl<'a> Default for #variant_generator_name<'a>
                where
                    #(#default_bounds,)*
                {
                    fn default() -> Self {
                        Self::new()
                    }
                }

                impl<'a> hegel::gen::Generate<#enum_name> for #variant_generator_name<'a> {
                    fn generate(&self) -> #enum_name {
                        use hegel::gen::Generate;
                        if let Some(basic) = self.as_basic() {
                            basic.generate_transformed()
                        } else {
                            #enum_name::#variant_name {
                                #(#field_constructions,)*
                            }
                        }
                    }

                    fn as_basic(&self) -> Option<hegel::gen::BasicGenerator<#enum_name>> {
                        use hegel::gen::Generate;

                        let variant_name_str = stringify!(#variant_name);
                        let field_name_strs: Vec<&str> = vec![#(stringify!(#field_names)),*];

                        // Get basic generators for all fields
                        #(
                            let #field_names = self.#field_names.as_basic()?;
                        )*

                        // Build inner object schema: {type: "object", properties: {field: schema, ...}, required: [...]}
                        let inner_schema = ciborium::Value::Map(vec![
                            (
                                ciborium::Value::Text("type".to_string()),
                                ciborium::Value::Text("object".to_string()),
                            ),
                            (
                                ciborium::Value::Text("properties".to_string()),
                                ciborium::Value::Map(vec![
                                    #((
                                        ciborium::Value::Text(stringify!(#field_names).to_string()),
                                        #field_names.schema.clone(),
                                    ),)*
                                ]),
                            ),
                            (
                                ciborium::Value::Text("required".to_string()),
                                ciborium::Value::Array(vec![
                                    #(ciborium::Value::Text(stringify!(#field_names).to_string()),)*
                                ]),
                            ),
                        ]);

                        // Wrap in outer object: {type: "object", properties: {VariantName: inner}, required: [VariantName]}
                        let outer_schema = ciborium::Value::Map(vec![
                            (
                                ciborium::Value::Text("type".to_string()),
                                ciborium::Value::Text("object".to_string()),
                            ),
                            (
                                ciborium::Value::Text("properties".to_string()),
                                ciborium::Value::Map(vec![
                                    (
                                        ciborium::Value::Text(variant_name_str.to_string()),
                                        inner_schema,
                                    ),
                                ]),
                            ),
                            (
                                ciborium::Value::Text("required".to_string()),
                                ciborium::Value::Array(vec![
                                    ciborium::Value::Text(variant_name_str.to_string()),
                                ]),
                            ),
                        ]);

                        // Capture transforms
                        #(
                            let #field_names = #field_names.transform;
                        )*

                        let field_names_owned: Vec<String> = field_name_strs.iter().map(|s| s.to_string()).collect();
                        let vname = variant_name_str.to_string();

                        Some(hegel::gen::BasicGenerator::with_transform(outer_schema, move |raw| {
                            // Unwrap outer map to get inner value
                            let mut outer_map: std::collections::HashMap<String, ciborium::Value> = match raw {
                                ciborium::Value::Map(entries) => {
                                    entries.into_iter().filter_map(|(k, v)| {
                                        if let ciborium::Value::Text(key) = k { Some((key, v)) } else { None }
                                    }).collect()
                                }
                                _ => panic!("hegel: expected object for enum variant, got {:?}", raw),
                            };

                            let inner_raw = outer_map.remove(&vname)
                                .unwrap_or_else(|| panic!("hegel: missing variant key '{}'", vname));

                            // Unwrap inner map to get fields
                            let mut inner_map: std::collections::HashMap<String, ciborium::Value> = match inner_raw {
                                ciborium::Value::Map(entries) => {
                                    entries.into_iter().filter_map(|(k, v)| {
                                        if let ciborium::Value::Text(key) = k { Some((key, v)) } else { None }
                                    }).collect()
                                }
                                _ => panic!("hegel: expected inner object for variant fields, got {:?}", inner_raw),
                            };

                            // Extract and transform each field
                            #(
                                let #field_names = {
                                    let fname = stringify!(#field_names);
                                    let raw_val = inner_map.remove(fname)
                                        .unwrap_or_else(|| panic!("hegel: missing field '{}'", fname));
                                    if let Some(ref t) = #field_names {
                                        t(raw_val)
                                    } else {
                                        hegel::gen::deserialize_value(raw_val)
                                    }
                                };
                            )*

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
                    value: hegel::gen::BoxedGenerator<'a, #field_type>,
                }

                impl<'a> #variant_generator_name<'a> {
                    /// Create a new generator with the default generator for the field.
                    pub fn new() -> Self
                    where
                        #field_type: hegel::gen::DefaultGenerator,
                        <#field_type as hegel::gen::DefaultGenerator>::Generator: Send + Sync + 'a,
                    {
                        Self {
                            value: <#field_type as hegel::gen::DefaultGenerator>::default_generator().boxed(),
                        }
                    }

                    /// Set a custom generator for the value.
                    pub fn with_value<G>(mut self, gen: G) -> Self
                    where
                        G: hegel::gen::Generate<#field_type> + Send + Sync + 'a,
                    {
                        self.value = gen.boxed();
                        self
                    }
                }

                impl<'a> Default for #variant_generator_name<'a>
                where
                    #field_type: hegel::gen::DefaultGenerator,
                    <#field_type as hegel::gen::DefaultGenerator>::Generator: Send + Sync + 'a,
                {
                    fn default() -> Self {
                        Self::new()
                    }
                }

                impl<'a> hegel::gen::Generate<#enum_name> for #variant_generator_name<'a> {
                    fn generate(&self) -> #enum_name {
                        use hegel::gen::Generate;
                        if let Some(basic) = self.as_basic() {
                            basic.generate_transformed()
                        } else {
                            #enum_name::#variant_name(self.value.generate())
                        }
                    }

                    fn as_basic(&self) -> Option<hegel::gen::BasicGenerator<#enum_name>> {
                        use hegel::gen::Generate;

                        let variant_name_str = stringify!(#variant_name);
                        let b_value = self.value.as_basic()?;

                        // Outer schema: {type: "object", properties: {VariantName: field_schema}, required: [VariantName]}
                        let outer_schema = ciborium::Value::Map(vec![
                            (
                                ciborium::Value::Text("type".to_string()),
                                ciborium::Value::Text("object".to_string()),
                            ),
                            (
                                ciborium::Value::Text("properties".to_string()),
                                ciborium::Value::Map(vec![
                                    (
                                        ciborium::Value::Text(variant_name_str.to_string()),
                                        b_value.schema.clone(),
                                    ),
                                ]),
                            ),
                            (
                                ciborium::Value::Text("required".to_string()),
                                ciborium::Value::Array(vec![
                                    ciborium::Value::Text(variant_name_str.to_string()),
                                ]),
                            ),
                        ]);

                        let t_value = b_value.transform;
                        let vname = variant_name_str.to_string();

                        Some(hegel::gen::BasicGenerator::with_transform(outer_schema, move |raw| {
                            // Unwrap outer map
                            let mut outer_map: std::collections::HashMap<String, ciborium::Value> = match raw {
                                ciborium::Value::Map(entries) => {
                                    entries.into_iter().filter_map(|(k, v)| {
                                        if let ciborium::Value::Text(key) = k { Some((key, v)) } else { None }
                                    }).collect()
                                }
                                _ => panic!("hegel: expected object for enum variant, got {:?}", raw),
                            };

                            let field_raw = outer_map.remove(&vname)
                                .unwrap_or_else(|| panic!("hegel: missing variant key '{}'", vname));

                            let field_val = if let Some(ref t) = t_value {
                                t(field_raw)
                            } else {
                                hegel::gen::deserialize_value(field_raw)
                            };

                            #enum_name::#variant_name(field_val)
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

            let with_methods: Vec<_> = field_indices
                .iter()
                .zip(field_types.iter())
                .map(|(field_idx, field_type)| {
                    let with_method_name = format_ident!("with{}", field_idx);
                    quote! {
                        /// Set a custom generator for this field.
                        pub fn #with_method_name<G>(mut self, gen: G) -> Self
                        where
                            G: hegel::gen::Generate<#field_type> + Send + Sync + 'a,
                        {
                            self.#field_idx = gen.boxed();
                            self
                        }
                    }
                })
                .collect();

            let generator_fields: Vec<_> = field_indices
                .iter()
                .zip(field_types.iter())
                .map(|(field_idx, field_type)| {
                    quote! { #field_idx: hegel::gen::BoxedGenerator<'a, #field_type> }
                })
                .collect();

            let new_inits: Vec<_> = field_indices
                .iter()
                .zip(field_types.iter())
                .map(|(field_idx, field_type)| {
                    quote! {
                        #field_idx: <#field_type as hegel::gen::DefaultGenerator>::default_generator().boxed()
                    }
                })
                .collect();

            let default_bounds: Vec<_> = field_types
                .iter()
                .map(|ty| {
                    quote! {
                        #ty: hegel::gen::DefaultGenerator,
                        <#ty as hegel::gen::DefaultGenerator>::Generator: Send + Sync + 'a
                    }
                })
                .collect();

            let field_generates: Vec<_> = field_indices
                .iter()
                .map(|field_idx| {
                    quote! { self.#field_idx.generate() }
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

                    #(#with_methods)*
                }

                impl<'a> Default for #variant_generator_name<'a>
                where
                    #(#default_bounds,)*
                {
                    fn default() -> Self {
                        Self::new()
                    }
                }

                impl<'a> hegel::gen::Generate<#enum_name> for #variant_generator_name<'a> {
                    fn generate(&self) -> #enum_name {
                        use hegel::gen::Generate;
                        if let Some(basic) = self.as_basic() {
                            basic.generate_transformed()
                        } else {
                            #enum_name::#variant_name(#(#field_generates,)*)
                        }
                    }

                    fn as_basic(&self) -> Option<hegel::gen::BasicGenerator<#enum_name>> {
                        use hegel::gen::Generate;

                        let variant_name_str = stringify!(#variant_name);

                        // Get basic generators for all tuple fields
                        #(
                            let #field_indices = self.#field_indices.as_basic()?;
                        )*

                        // Inner schema: {type: "tuple", elements: [schema0, schema1, ...]}
                        let inner_schema = ciborium::Value::Map(vec![
                            (
                                ciborium::Value::Text("type".to_string()),
                                ciborium::Value::Text("tuple".to_string()),
                            ),
                            (
                                ciborium::Value::Text("elements".to_string()),
                                ciborium::Value::Array(vec![
                                    #(#field_indices.schema.clone(),)*
                                ]),
                            ),
                        ]);

                        // Outer schema: {type: "object", properties: {VariantName: inner}, required: [VariantName]}
                        let outer_schema = ciborium::Value::Map(vec![
                            (
                                ciborium::Value::Text("type".to_string()),
                                ciborium::Value::Text("object".to_string()),
                            ),
                            (
                                ciborium::Value::Text("properties".to_string()),
                                ciborium::Value::Map(vec![
                                    (
                                        ciborium::Value::Text(variant_name_str.to_string()),
                                        inner_schema,
                                    ),
                                ]),
                            ),
                            (
                                ciborium::Value::Text("required".to_string()),
                                ciborium::Value::Array(vec![
                                    ciborium::Value::Text(variant_name_str.to_string()),
                                ]),
                            ),
                        ]);

                        // Capture transforms
                        #(
                            let #field_indices = #field_indices.transform;
                        )*

                        let vname = variant_name_str.to_string();

                        Some(hegel::gen::BasicGenerator::with_transform(outer_schema, move |raw| {
                            // Unwrap outer map
                            let mut outer_map: std::collections::HashMap<String, ciborium::Value> = match raw {
                                ciborium::Value::Map(entries) => {
                                    entries.into_iter().filter_map(|(k, v)| {
                                        if let ciborium::Value::Text(key) = k { Some((key, v)) } else { None }
                                    }).collect()
                                }
                                _ => panic!("hegel: expected object for enum variant, got {:?}", raw),
                            };

                            let tuple_raw = outer_map.remove(&vname)
                                .unwrap_or_else(|| panic!("hegel: missing variant key '{}'", vname));

                            // Extract array elements
                            let arr = match tuple_raw {
                                ciborium::Value::Array(arr) => arr,
                                _ => panic!("hegel: expected array for tuple variant, got {:?}", tuple_raw),
                            };
                            let mut iter = arr.into_iter();

                            #(
                                let #field_indices = {
                                    let raw_val = iter.next()
                                        .unwrap_or_else(|| panic!("hegel: tuple variant missing element"));
                                    if let Some(ref t) = #field_indices {
                                        t(raw_val)
                                    } else {
                                        hegel::gen::deserialize_value(raw_val)
                                    }
                                };
                            )*

                            #enum_name::#variant_name(#(#field_indices,)*)
                        }))
                    }
                }
            }
        }
    }
}
