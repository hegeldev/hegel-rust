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

    // Generate the generate() implementation
    let generate_fields = field_names.iter().map(|name| {
        quote! {
            #name: self.#name.generate()
        }
    });

    // Generate schema() implementation
    let schema_fields = field_names.iter().map(|name| {
        let name_str = name.to_string();
        quote! {
            {
                let field_schema = self.#name.schema()?;
                properties.insert(#name_str.to_string(), field_schema);
                required.push(serde_json::json!(#name_str));
            }
        }
    });

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
                #name {
                    #(#generate_fields,)*
                }
            }

            fn schema(&self) -> Option<serde_json::Value> {
                use hegel::gen::Generate;

                let mut properties = serde_json::Map::new();
                let mut required = Vec::new();

                #(#schema_fields)*

                Some(serde_json::json!({
                    "type": "object",
                    "properties": properties,
                    "required": required
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

    // Separate unit variants from data variants
    let unit_variants: Vec<_> = variants
        .iter()
        .filter(|v| matches!(classify_variant(v), VariantKind::Unit))
        .collect();

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

    // Generate variant names for schema and generate
    let all_variant_names: Vec<_> = variants.iter().map(|v| v.ident.to_string()).collect();

    // Generate schema entries for unit variants
    let unit_schema_entries: Vec<_> = unit_variants
        .iter()
        .map(|variant| {
            let variant_name_str = variant.ident.to_string();
            quote! { serde_json::json!({ "const": #variant_name_str }) }
        })
        .collect();

    // Generate schema composition for data variants
    let data_schema_entries: Vec<_> = data_variants
        .iter()
        .map(|variant| {
            let variant_name = &variant.ident;
            quote! {
                self.#variant_name.schema()?
            }
        })
        .collect();

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

    // Generate the schema() implementation
    let schema_impl = if data_variants.is_empty() {
        // All unit variants - pure enum schema
        quote! {
            fn schema(&self) -> Option<serde_json::Value> {
                let variants: Vec<&str> = vec![#(#all_variant_names),*];
                Some(serde_json::json!({ "enum": variants }))
            }
        }
    } else {
        quote! {
            fn schema(&self) -> Option<serde_json::Value> {
                use hegel::gen::Generate;

                let mut one_of = Vec::new();
                #(one_of.push(#unit_schema_entries);)*
                #(one_of.push(#data_schema_entries);)*
                Some(serde_json::json!({ "one_of": one_of }))
            }
        }
    };

    // Generate the generate() implementation
    let generate_impl = if data_variants.is_empty() {
        // All unit variants - just sample from names
        quote! {
            fn generate(&self) -> #enum_name {
                let variants: Vec<&str> = vec![#(#all_variant_names),*];
                let selected: String = hegel::gen::generate_from_schema(
                    &serde_json::json!({ "enum": variants })
                );

                match selected.as_str() {
                    #(#generate_match_arms,)*
                    _ => unreachable!("Unknown variant: {}", selected),
                }
            }
        }
    } else {
        quote! {
            fn generate(&self) -> #enum_name {
                use hegel::gen::Generate;

                if let Some(schema) = self.schema() {
                    // All variants have schemas - single round trip
                    hegel::gen::generate_from_schema(&schema)
                } else {
                    // Compositional fallback with grouping
                    hegel::gen::group(hegel::gen::labels::ENUM_VARIANT, || {
                        let variants: Vec<&str> = vec![#(#all_variant_names),*];
                        let selected: String = hegel::gen::generate_from_schema(
                            &serde_json::json!({ "enum": variants })
                        );

                        match selected.as_str() {
                            #(#generate_match_arms,)*
                            _ => unreachable!("Unknown variant: {}", selected),
                        }
                    })
                }
            }
        }
    };

    let generate_trait_impl = if data_variants.is_empty() {
        quote! {
            impl hegel::gen::Generate<#enum_name> for #generator_name {
                #generate_impl
                #schema_impl
            }
        }
    } else {
        quote! {
            impl<'a> hegel::gen::Generate<#enum_name> for #generator_name<'a> {
                #generate_impl
                #schema_impl
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
    let variant_name_str = variant_name.to_string();
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

            // Generate schema properties
            let schema_properties: Vec<_> = field_names
                .iter()
                .map(|field_name| {
                    let field_name_str = field_name.to_string();
                    quote! {
                        {
                            let field_schema = self.#field_name.schema()?;
                            inner_properties.insert(#field_name_str.to_string(), field_schema);
                            inner_required.push(serde_json::json!(#field_name_str));
                        }
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
                        #enum_name::#variant_name {
                            #(#field_constructions,)*
                        }
                    }

                    fn schema(&self) -> Option<serde_json::Value> {
                        use hegel::gen::Generate;

                        let mut inner_properties = serde_json::Map::new();
                        let mut inner_required = Vec::new();

                        #(#schema_properties)*

                        let inner_schema = serde_json::json!({
                            "type": "object",
                            "properties": inner_properties,
                            "required": inner_required
                        });

                        let mut outer_properties = serde_json::Map::new();
                        outer_properties.insert(#variant_name_str.to_string(), inner_schema);

                        Some(serde_json::json!({
                            "type": "object",
                            "properties": outer_properties,
                            "required": [#variant_name_str]
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
                        #enum_name::#variant_name(self.value.generate())
                    }

                    fn schema(&self) -> Option<serde_json::Value> {
                        use hegel::gen::Generate;

                        let inner_schema = self.value.schema()?;

                        let mut outer_properties = serde_json::Map::new();
                        outer_properties.insert(#variant_name_str.to_string(), inner_schema);

                        Some(serde_json::json!({
                            "type": "object",
                            "properties": outer_properties,
                            "required": [#variant_name_str]
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

            let schema_items: Vec<_> = field_indices
                .iter()
                .map(|field_idx| {
                    quote! { self.#field_idx.schema()? }
                })
                .collect();

            let num_fields = field_types.len();

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
                        #enum_name::#variant_name(#(#field_generates,)*)
                    }

                    fn schema(&self) -> Option<serde_json::Value> {
                        use hegel::gen::Generate;

                        let elements: Vec<serde_json::Value> = vec![#(#schema_items,)*];

                        let inner_schema = serde_json::json!({
                            "type": "tuple",
                            "elements": elements
                        });

                        let mut outer_properties = serde_json::Map::new();
                        outer_properties.insert(#variant_name_str.to_string(), inner_schema);

                        Some(serde_json::json!({
                            "type": "object",
                            "properties": outer_properties,
                            "required": [#variant_name_str]
                        }))
                    }
                }
            }
        }
    }
}
