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
            // Forward the method's attributes (cfg gates, user attribute
            // macros, ...) onto the generated vec entry, except doc
            // comments: those would land on an expression and trip
            // unused_doc_comments in the user's crate.
            let attrs: Vec<&Attribute> = m
                .attrs
                .iter()
                .filter(|a| !a.path().is_ident("doc"))
                .collect();
            // Register through a non-capturing closure rather than
            // `Self::#name` directly: `Rule.apply` is `fn(&mut M, TestCase)`,
            // and the method-call syntax inside the closure lets methods take
            // either `&self` or `&mut self` (an `&mut M` auto-coerces to
            // `&M`), as the `stateful` module docs promise for invariants.
            quote! {
                #(#attrs)*
                ::hegel::stateful::Rule::new(
                    #name_str,
                    |__hegel_machine: &mut Self, __hegel_tc: ::hegel::TestCase| {
                        __hegel_machine.#name(__hegel_tc)
                    },
                )
            }
        })
        .collect()
}

/// The concurrency group of one `#[rule]` in a concurrent state machine:
/// the string from `#[rule(group = "...")]`, or the shared anonymous group
/// for a bare `#[rule]`.
enum RuleGroup {
    Anonymous,
    Named(String),
}

/// Extract the `group = "..."` argument of a `#[rule]` attribute, if any.
fn rule_group(attr: &Attribute) -> syn::Result<RuleGroup> {
    if matches!(attr.meta, syn::Meta::Path(_)) {
        return Ok(RuleGroup::Anonymous);
    }
    let mut group = None;
    attr.parse_nested_meta(|meta| {
        if meta.path.is_ident("group") {
            let value: syn::LitStr = meta.value()?.parse()?;
            group = Some(value.value());
            Ok(())
        } else {
            Err(meta.error("unsupported #[rule] argument; expected `group = \"...\"`"))
        }
    })?;
    match group {
        Some(group) => Ok(RuleGroup::Named(group)),
        None => Err(syn::Error::new_spanned(
            attr,
            "#[rule(...)] requires `group = \"...\"`",
        )),
    }
}

struct ConcurrentMethodInfo {
    name: syn::Ident,
    group: Option<RuleGroup>,
    attrs: Vec<Attribute>,
}

fn concurrent_method_entries(methods: &[ConcurrentMethodInfo]) -> Vec<TokenStream> {
    methods
        .iter()
        .map(|m| {
            let name_str = m.name.to_string();
            let name = &m.name;
            let attrs: Vec<&Attribute> = m
                .attrs
                .iter()
                .filter(|a| !a.path().is_ident("doc"))
                .collect();
            match &m.group {
                Some(group) => {
                    let group = match group {
                        RuleGroup::Anonymous => quote! { ::hegel::stateful::ANONYMOUS_GROUP },
                        RuleGroup::Named(name) => quote! { #name },
                    };
                    quote! {
                        #(#attrs)*
                        ::hegel::stateful::ConcurrentRule::new(
                            #name_str,
                            #group,
                            |__hegel_machine: &Self, __hegel_tc: ::hegel::TestCase| {
                                __hegel_machine.#name(__hegel_tc)
                            },
                        )
                    }
                }
                None => quote! {
                    #(#attrs)*
                    ::hegel::stateful::ConcurrentInvariant::new(
                        #name_str,
                        |__hegel_machine: &Self, __hegel_tc: ::hegel::TestCase| {
                            __hegel_machine.#name(__hegel_tc)
                        },
                    )
                },
            }
        })
        .collect()
}

pub fn expand_concurrent_state_machine(mut block: ItemImpl) -> TokenStream {
    let mut rules = Vec::new();
    let mut invariants = Vec::new();

    for item in &mut block.items {
        if let ImplItem::Fn(method) = item {
            let rule_attr = method.attrs.iter().find(|a| is_rule(a)).cloned();
            let has_invariant = method.attrs.iter().any(&is_invariant);
            method.attrs.retain(|a| !is_rule(a) && !is_invariant(a));

            if rule_attr.is_some() || has_invariant {
                let takes_shared_self = method.sig.receiver().is_some_and(|receiver| {
                    matches!(&*receiver.ty, syn::Type::Reference(r) if r.mutability.is_none())
                });
                if !takes_shared_self {
                    return syn::Error::new_spanned(
                        &method.sig,
                        "#[rule] and #[invariant] methods in a concurrent state machine must \
                         take `&self`: the model is shared by reference across worker threads, \
                         so mutable state needs interior mutability (locks, atomics, ...)",
                    )
                    .to_compile_error();
                }
            }

            if let Some(attr) = rule_attr {
                let group = match rule_group(&attr) {
                    Ok(group) => group,
                    Err(e) => return e.to_compile_error(),
                };
                rules.push(ConcurrentMethodInfo {
                    name: method.sig.ident.clone(),
                    group: Some(group),
                    attrs: method.attrs.clone(),
                });
            }
            if has_invariant {
                invariants.push(ConcurrentMethodInfo {
                    name: method.sig.ident.clone(),
                    group: None,
                    attrs: method.attrs.clone(),
                });
            }
        }
    }

    let block_type = &block.self_ty;
    let (impl_generics, _, where_clause) = block.generics.split_for_impl();
    let rule_entries = concurrent_method_entries(&rules);
    let invariant_entries = concurrent_method_entries(&invariants);

    quote! {
        #block
        impl #impl_generics ::hegel::stateful::ConcurrentStateMachine for #block_type #where_clause {
            fn rules(&self) -> ::std::vec::Vec<::hegel::stateful::ConcurrentRule<Self>> {
                ::std::vec![ #( #rule_entries ),* ]
            }
            fn invariants(&self) -> ::std::vec::Vec<::hegel::stateful::ConcurrentInvariant<Self>> {
                ::std::vec![ #( #invariant_entries ),* ]
            }
        }
    }
}

pub fn expand_state_machine(mut block: ItemImpl) -> TokenStream {
    let mut rules = Vec::new();
    let mut invariants = Vec::new();

    for item in &mut block.items {
        if let ImplItem::Fn(method) = item {
            let has_rule = method.attrs.iter().any(&is_rule);
            let has_invariant = method.attrs.iter().any(&is_invariant);
            method.attrs.retain(|a| !is_rule(a) && !is_invariant(a));

            // Rules and invariants are applied through a `&mut M` handle, so
            // the method must borrow `self` (`&self` or `&mut self`,
            // including the explicit `self: &Self` / `self: &mut Self`
            // forms). Reject by-value receivers here with a targeted error:
            // for a `Copy` state machine, `m.rule(tc)` on a by-value receiver
            // would otherwise compile and silently mutate a copy.
            if has_rule || has_invariant {
                let borrows_self = method.sig.receiver().is_some_and(|receiver| {
                    receiver.reference.is_some() || matches!(&*receiver.ty, syn::Type::Reference(_))
                });
                if !borrows_self {
                    return syn::Error::new_spanned(
                        &method.sig,
                        "#[rule] and #[invariant] methods must take `&self` or `&mut self`",
                    )
                    .to_compile_error();
                }
            }

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
            fn rules(&self) -> ::std::vec::Vec<::hegel::stateful::Rule<Self>> {
                ::std::vec![ #( #rule_entries ),* ]
            }
            fn invariants(&self) -> ::std::vec::Vec<::hegel::stateful::Rule<Self>> {
                ::std::vec![ #( #invariant_entries ),* ]
            }
        }
    }
}
