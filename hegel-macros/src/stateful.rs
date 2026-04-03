use proc_macro2::TokenStream;
use quote::quote;
use syn::{Attribute, ImplItem, ItemImpl};

fn is_rule(a: &Attribute) -> bool {
    a.path().is_ident("rule")
}

fn is_invariant(a: &Attribute) -> bool {
    a.path().is_ident("invariant")
}

struct MethodInfo {
    name: syn::Ident,
    attrs: Vec<Attribute>,
}

fn method_entries(methods: &[MethodInfo]) -> Vec<TokenStream> {
    methods
        .iter()
        .map(|m| {
            let name_str = m.name.to_string();
            let name = &m.name;
            let attrs = &m.attrs;
            quote! {
                #(#attrs)*
                ::hegel::stateful::Rule::new(#name_str, Self::#name)
            }
        })
        .collect()
}

pub fn expand_state_machine(mut block: ItemImpl) -> TokenStream {
    let mut rules = Vec::new();
    let mut invariants = Vec::new();

    for item in &mut block.items {
        if let ImplItem::Fn(method) = item {
            let has_rule = method.attrs.iter().any(&is_rule);
            let has_invariant = method.attrs.iter().any(&is_invariant);
            method.attrs.retain(|a| !is_rule(a) && !is_invariant(a));

            let info = || MethodInfo {
                name: method.sig.ident.clone(),
                attrs: method.attrs.clone(),
            };
            if has_rule {
                rules.push(info());
            }
            if has_invariant {
                invariants.push(info());
            }
        }
    }

    let block_type = &block.self_ty;
    let (impl_generics, _, where_clause) = block.generics.split_for_impl();
    let rule_entries = method_entries(&rules);
    let invariant_entries = method_entries(&invariants);

    quote! {
        #block
        impl #impl_generics ::hegel::stateful::StateMachine for #block_type #where_clause {
            fn rules(&self) -> Vec<::hegel::stateful::Rule<Self>> {
                vec![ #( #rule_entries ),* ]
            }
            fn invariants(&self) -> Vec<::hegel::stateful::Rule<Self>> {
                vec![ #( #invariant_entries ),* ]
            }
        }
    }
}
