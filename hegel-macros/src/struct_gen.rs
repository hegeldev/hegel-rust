use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{DeriveInput, Fields};

use crate::utils::{cbor_to_iter, default_or_custom_gen_bounds, tuple_schema};

/// Derive Generator for a struct.
pub(crate) fn derive_struct_generator(input: &DeriveInput, data: &syn::DataStruct) -> TokenStream {
    let name = &input.ident;
    let generator_name = format_ident!("{}Generator", name);

    let fields = match &data.fields {
        Fields::Named(fields) => &fields.named,
        Fields::Unnamed(_) => {
            return syn::Error::new_spanned(
                input,
                "Generator can only be derived for structs with named fields",
            )
            .to_compile_error()
            .into();
        }
        Fields::Unit => {
            return syn::Error::new_spanned(input, "Generator cannot be derived for unit structs")
                .to_compile_error()
                .into();
        }
    };

    let field_names: Vec<_> = fields.iter().map(|f| f.ident.as_ref().unwrap()).collect();

    let field_types: Vec<_> = fields.iter().map(|f| &f.ty).collect();

    let field_custom_gens:Vec<_> = fields.iter().map(|f| {
        let mut generate_with_path = None;
        for attr in &f.attrs {
            if attr.path().is_ident("hegel") {
                let _ = attr.parse_nested_meta(|meta| {
                    if meta.path.is_ident("generate_with") {
                        let value: syn::LitStr = meta.value()?.parse()?;
                        generate_with_path = Some(value.parse()?);
                    }
                    Ok(())
                });
            }
        }
        generate_with_path
    }).collect();

    // Generate the builder method names (same as field names, no prefix)
    let builder_methods: Vec<_> = field_names.to_vec();

    // Generate field definitions for the generator struct
    let generator_fields = field_names
        .iter()
        .zip(field_types.iter())
        .map(|(name, ty)| {
            quote! {
                #name: hegel::generators::BoxedGenerator<'a, #ty>
            }
        });

    // Generate the new() constructor
    let new_field_inits = field_types.iter()
    .zip(field_custom_gens.iter())
    .map(|(ty, path_opt)| {
        if let Some(path) = path_opt {
            quote! {
                #path().boxed()
            }
        } else {
            quote! {
                <#ty as hegel::generators::DefaultGenerator>::default_generator().boxed()
            }
        }
    });

    let new_fields = field_names.iter().zip(new_field_inits).map(|(name, init)| {
        quote! { #name: #init }
    });

    // Generator Default trait bounds for new()
    let default_bounds = default_or_custom_gen_bounds(&field_types, &field_custom_gens, quote! { 'a });

    // Generate builder methods
    let with_method_impls = field_names
        .iter()
        .zip(field_types.iter())
        .zip(builder_methods.iter())
        .map(|((field_name, field_type), method_name)| {
            quote! {
                /// Set a custom generator for this field.
                pub fn #method_name<G>(mut self, generator: G) -> Self
                where
                    G: hegel::generators::Generator<#field_type> + Send + Sync + 'a,
                {
                    self.#field_name = generator.boxed();
                    self
                }
            }
        });

    // Generate the do_draw() fallback fields
    let generate_fields = field_names.iter().map(|name| {
        quote! {
            #name: self.#name.do_draw(__tc)
        }
    });

    // Generate per-field basic bindings: let basic_field = self.field.as_basic()?;
    let basic_bindings: Vec<proc_macro2::TokenStream> = field_names
        .iter()
        .map(|name| {
            let basic_name = format_ident!("basic_{}", name);
            quote! { let #basic_name = self.#name.as_basic()?; }
        })
        .collect();

    // Generate schema elements from basics (positional, in field order)
    let schema_elements: Vec<_> = field_names
        .iter()
        .map(|name| {
            let basic_name = format_ident!("basic_{}", name);
            quote! { #basic_name.schema().clone() }
        })
        .collect();

    // Generate per-field extraction in parse closure (positional from tuple)
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

    let construct_fields: Vec<&syn::Ident> = field_names.clone();

    // Generator DefaultGenerate bounds (same as new() but with 'static lifetime)
    let default_generator_bounds = default_or_custom_gen_bounds(&field_types, &field_custom_gens, quote! { 'static });

    let schema_ts = tuple_schema(schema_elements);
    let parse_iter_ts = cbor_to_iter("iter", quote! { raw }, "Expected tuple from struct schema");

    let expanded = quote! {
        const _: () = {
            use hegel::generators::Generator as _;

            pub struct #generator_name<'a> {
                #(#generator_fields,)*
            }

            impl<'a> #generator_name<'a> {
                pub fn new() -> Self
                where
                    #(#default_bounds),*
                {
                    Self {
                        #(#new_fields,)*
                    }
                }

                #(#with_method_impls)*
            }

            impl<'a> Default for #generator_name<'a>
            where
                #(#default_bounds),*
            {
                fn default() -> Self {
                    Self::new()
                }
            }

            impl<'a> hegel::generators::Generator<#name> for #generator_name<'a> {
                fn do_draw(&self, __tc: &hegel::TestCase) -> #name {
                    if let Some(basic) = self.as_basic() {
                        basic.parse_raw(hegel::generate_raw(__tc, basic.schema()))
                    } else {
                        __tc.start_span(hegel::generators::labels::FIXED_DICT);
                        let __result = #name {
                            #(#generate_fields,)*
                        };
                        __tc.stop_span(false);
                        __result
                    }
                }

                fn as_basic(&self) -> Option<hegel::generators::BasicGenerator<'_, #name>> {
                    #(#basic_bindings)*

                    let schema = #schema_ts;

                    Some(hegel::generators::BasicGenerator::new(schema, move |raw| {
                        #parse_iter_ts

                        #(#field_parse_in_closure)*

                        #name {
                            #(#construct_fields,)*
                        }
                    }))
                }
            }

            impl hegel::generators::DefaultGenerator for #name
            where
                #(#default_generator_bounds),*
            {
                type Generator = #generator_name<'static>;
                fn default_generator() -> Self::Generator {
                    #generator_name::new()
                }
            }
        };
    };

    TokenStream::from(expanded)
}
