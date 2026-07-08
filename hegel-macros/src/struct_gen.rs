use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{DeriveInput, Fields};

use crate::utils::{GenericsParts, default_gen_bounds, split_generics};

/// Derive Generator for a struct.
pub(crate) fn derive_struct_generator(input: &DeriveInput, data: &syn::DataStruct) -> TokenStream {
    let name = &input.ident;
    let generator_name = format_ident!("{}Generator", name);

    if data.fields.is_empty() {
        return syn::Error::new_spanned(
            name,
            "DefaultGenerator cannot be derived for structs with no fields: \
             there is nothing to configure; use gs::just(...) instead",
        )
        .to_compile_error()
        .into();
    }

    let (field_names, is_tuple): (Vec<syn::Ident>, bool) = match &data.fields {
        Fields::Named(fields) => {
            let names: Vec<_> = fields
                .named
                .iter()
                .map(|f| f.ident.as_ref().unwrap().clone())
                .collect();
            for field_name in &names {
                if *field_name == "new" || *field_name == "boxed" || *field_name == "__phantom" {
                    return syn::Error::new_spanned(
                        field_name,
                        format!(
                            "field name `{field_name}` collides with the generated builder API \
                             of #[derive(DefaultGenerator)]; rename the field or implement \
                             DefaultGenerator by hand"
                        ),
                    )
                    .to_compile_error()
                    .into();
                }
            }
            (names, false)
        }
        Fields::Unnamed(fields) => {
            let names = (0..fields.unnamed.len())
                .map(|i| format_ident!("_{}", i))
                .collect();
            (names, true)
        }
        Fields::Unit => unreachable!(),
    };

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

    let field_types: Vec<_> = data.fields.iter().map(|f| &f.ty).collect();

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

    let with_method_impls =
        field_names
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
            });

    let (span_label, construct) = if is_tuple {
        let draws = field_names.iter().map(|name| {
            quote! { self.#name.do_draw(__tc) }
        });
        (
            quote! { ::hegel::generators::labels::TUPLE },
            quote! { #name(#(#draws,)*) },
        )
    } else {
        let generate_fields = field_names.iter().map(|name| {
            quote! { #name: self.#name.do_draw(__tc) }
        });
        (
            quote! { ::hegel::generators::labels::FIXED_DICT },
            quote! { #name { #(#generate_fields,)* } },
        )
    };

    let default_generator_bounds = default_gen_bounds(&field_types, quote! { 'static });

    let expanded = quote! {
        const _: () = {
            use ::hegel::generators::Generator as _;

            pub struct #generator_name<'a, #gen_params>
            where
                #(#user_predicates,)*
                #(#type_param_idents: 'a,)*
            {
                #(#generator_fields,)*
                #phantom_field
            }

            impl<'a, #gen_params> #generator_name<'a, #(#param_uses,)*>
            where
                #(#user_predicates,)*
                #(#type_param_idents: 'a,)*
            {
                pub fn new() -> Self
                where
                    #(#default_bounds,)*
                {
                    Self {
                        #(#new_fields,)*
                        #phantom_init
                    }
                }

                #(#with_method_impls)*
            }

            impl<'a, #gen_params> Default for #generator_name<'a, #(#param_uses,)*>
            where
                #(#user_predicates,)*
                #(#type_param_idents: 'a,)*
                #(#default_bounds,)*
            {
                fn default() -> Self {
                    Self::new()
                }
            }

            impl<'a, #gen_params> ::hegel::generators::Generator<#self_ty>
                for #generator_name<'a, #(#param_uses,)*>
            where
                #(#user_predicates,)*
                #(#type_param_idents: 'a,)*
            {
                fn do_draw(&self, __tc: &::hegel::TestCase) -> #self_ty {
                    __tc.start_span(#span_label);
                    let __result = #construct;
                    __tc.stop_span(false);
                    __result
                }
            }

            impl<'a, #gen_params> ::hegel::generators::PrintableGenerator<#self_ty>
                for #generator_name<'a, #(#param_uses,)*>
            where
                #(#user_predicates,)*
                #(#type_param_idents: 'a,)*
                #self_ty: ::hegel::PrettyPrintable,
            {
                fn do_draw_and_print(
                    &self,
                    __tc: &::hegel::TestCase,
                    __printer: &mut ::hegel::PrettyPrinter,
                ) -> #self_ty {
                    let __value = ::hegel::generators::Generator::do_draw(self, __tc);
                    ::hegel::PrettyPrintable::pretty_print(&__value, __printer);
                    __value
                }
            }

            impl<#gen_params> ::hegel::generators::DefaultGenerator for #self_ty
            where
                #(#user_predicates,)*
                #(#type_param_idents: 'static,)*
                #(#default_generator_bounds,)*
            {
                type Generator = #generator_name<'static, #(#param_uses,)*>;
                fn default_generator() -> Self::Generator {
                    #generator_name::new()
                }
            }
        };
    };

    TokenStream::from(expanded)
}
