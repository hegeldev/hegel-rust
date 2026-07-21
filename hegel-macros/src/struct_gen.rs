use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{DeriveInput, Fields};

use crate::utils::{
    GenericsParts, default_gen_bounds, generator_param_ident, print_shape, split_generics,
};

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
        if *field_name == "new" || *field_name == "boxed" || *field_name == "__phantom" {
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
    let self_ty = quote! { #name #ty_generics };
    let gen_params = if gen_params.is_empty() {
        quote! {}
    } else {
        quote! { #gen_params, }
    };
    let phantom_field = if input.generics.params.is_empty() {
        quote! {}
    } else {
        quote! { __phantom: ::core::marker::PhantomData<fn() -> #self_ty>, }
    };
    let phantom_init = if input.generics.params.is_empty() {
        quote! {}
    } else {
        quote! { __phantom: ::core::marker::PhantomData, }
    };

    let field_types: Vec<_> = fields.iter().map(|f| &f.ty).collect();
    let mut generator_params: Vec<syn::Ident> = field_names
        .iter()
        .map(|field_name| generator_param_ident(&field_name.to_string()))
        .collect();
    {
        let mut seen = std::collections::HashSet::new();
        if generator_params
            .iter()
            .any(|param| !seen.insert(param.to_string()))
        {
            generator_params = (0..field_names.len())
                .map(|i| format_ident!("__G{}", i))
                .collect();
        }
    }

    let generator_param_decls = generator_params
        .iter()
        .zip(field_types.iter())
        .map(|(param, ty)| {
            quote! {
                #param = <#ty as ::hegel::generators::DefaultGenerator>::Generator
            }
        });

    let generator_fields = field_names
        .iter()
        .zip(generator_params.iter())
        .map(|(name, param)| {
            quote! { #name: #param }
        });

    let new_fields = field_names.iter().zip(field_types.iter()).map(|(name, ty)| {
        quote! {
            #name: <#ty as ::hegel::generators::DefaultGenerator>::default_generator()
        }
    });

    let default_bounds = default_gen_bounds(&field_types);

    let with_method_impls = field_names.iter().enumerate().map(|(index, field_name)| {
        let field_type = field_types[index];
        let params_before = &generator_params[..index];
        let params_after = &generator_params[index + 1..];
        let moves = field_names
            .iter()
            .zip(generator_params.iter())
            .enumerate()
            .map(|(other, (other_name, _))| {
                if other == index {
                    quote! { #field_name: generator }
                } else {
                    quote! { #other_name: self.#other_name }
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
            pub fn #field_name<G>(
                self,
                generator: G,
            ) -> #generator_name<#(#param_uses,)* #(#params_before,)* G, #(#params_after,)*>
            where
                G: ::hegel::generators::Generator<#field_type>,
            {
                #generator_name {
                    #(#moves,)*
                    #phantom_init
                }
            }
        }
    });

    let generate_fields = field_names.iter().map(|name| {
        quote! {
            #name: self.#name.do_draw(__tc)
        }
    });

    let print_actions: Vec<_> = field_names
        .iter()
        .map(|field_name| {
            quote! {
                let #field_name = self.#field_name.draw_and_print(__tc, __printer);
            }
        })
        .collect();
    let print_body = print_shape(&name.to_string(), &data.fields, &print_actions);

    let expanded = quote! {
        #[allow(non_camel_case_types)]
        const _: () = {
            use ::hegel::generators::Generator as _;
            use ::hegel::generators::PrintableGenerator as _;

            pub struct #generator_name<#gen_params #(#generator_param_decls,)*>
            where
                #(#user_predicates,)*
            {
                #(#generator_fields,)*
                #phantom_field
            }

            impl<#gen_params> #generator_name<#(#param_uses,)*>
            where
                #(#user_predicates,)*
                #(#default_bounds,)*
            {
                pub fn new() -> Self {
                    Self {
                        #(#new_fields,)*
                        #phantom_init
                    }
                }
            }

            impl<#gen_params #(#generator_params,)*>
                #generator_name<#(#param_uses,)* #(#generator_params,)*>
            where
                #(#user_predicates,)*
            {
                #(#with_method_impls)*
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

            impl<#gen_params #(#generator_params,)*> ::hegel::generators::Generator<#self_ty>
                for #generator_name<#(#param_uses,)* #(#generator_params,)*>
            where
                #(#user_predicates,)*
                #(#generator_params: ::hegel::generators::Generator<#field_types>,)*
            {
                fn do_draw(&self, __tc: &::hegel::TestCase) -> #self_ty {
                    __tc.start_span(::hegel::generators::labels::FIXED_DICT);
                    let __result = #name {
                        #(#generate_fields,)*
                    };
                    __tc.stop_span(false);
                    __result
                }
            }

            impl<#gen_params #(#generator_params,)*>
                ::hegel::generators::PrintableGenerator<#self_ty>
                for #generator_name<#(#param_uses,)* #(#generator_params,)*>
            where
                #(#user_predicates,)*
                #(#generator_params: ::hegel::generators::PrintableGenerator<#field_types>,)*
            {
                fn do_draw_and_print(
                    &self,
                    __tc: &::hegel::TestCase,
                    __printer: &mut ::hegel::PrettyPrinter,
                ) -> #self_ty {
                    __tc.start_span(::hegel::generators::labels::FIXED_DICT);
                    #print_body
                    let __result = #name {
                        #(#field_names,)*
                    };
                    __tc.stop_span(false);
                    __result
                }
            }

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
    };

    TokenStream::from(expanded)
}
