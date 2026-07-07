use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{DeriveInput, Fields};

use crate::utils::default_gen_bounds;

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
    for field_name in &field_names {
        if *field_name == "new" || *field_name == "boxed" {
            return syn::Error::new_spanned(
                field_name,
                format!(
                    "field name `{field_name}` collides with the generated builder API of \
                     #[derive(DefaultGenerator)]; rename the field or implement \
                     DefaultGenerator by hand"
                ),
            )
            .to_compile_error()
            .into();
        }
    }

    let field_types: Vec<_> = fields.iter().map(|f| &f.ty).collect();

    let generator_fields = field_names
        .iter()
        .zip(field_types.iter())
        .map(|(name, ty)| {
            quote! {
                #name: ::hegel::generators::BoxedGenerator<'a, #ty>
            }
        });

    let new_field_inits = field_types.iter().map(|ty| {
        quote! {
            <#ty as ::hegel::generators::DefaultGenerator>::default_generator().boxed()
        }
    });

    let new_fields = field_names.iter().zip(new_field_inits).map(|(name, init)| {
        quote! { #name: #init }
    });

    let default_bounds = default_gen_bounds(&field_types, quote! { 'a });

    let with_method_impls = field_names.iter().zip(field_types.iter()).map(
        |(field_name, field_type)| {
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
        },
    );

    let generate_fields = field_names.iter().map(|name| {
        quote! {
            #name: self.#name.do_draw(__tc)
        }
    });

    let default_generator_bounds = default_gen_bounds(&field_types, quote! { 'static });

    let expanded = quote! {
        const _: () = {
            use ::hegel::generators::Generator as _;

            pub struct #generator_name<'a> {
                #(#generator_fields,)*
            }

            impl<'a> #generator_name<'a> {
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
                #(#default_bounds,)*
            {
                fn default() -> Self {
                    Self::new()
                }
            }

            impl<'a> ::hegel::generators::Generator<#name> for #generator_name<'a> {
                fn do_draw(&self, __tc: &::hegel::TestCase) -> #name {
                    __tc.start_span(::hegel::generators::labels::FIXED_DICT);
                    let __result = #name {
                        #(#generate_fields,)*
                    };
                    __tc.stop_span(false);
                    __result
                }
            }

            impl ::hegel::generators::DefaultGenerator for #name
            where
                #(#default_generator_bounds,)*
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
