use proc_macro2::TokenStream;
use quote::quote;
use syn::{Attribute, ImplItem, ItemImpl};

use crate::common::{extract_ident_from_pat, rewrite_draws_in_block};

fn is_rule(a: &Attribute) -> bool {
    a.path().is_ident("rule")
}

fn is_invariant(a: &Attribute) -> bool {
    a.path().is_ident("invariant")
}

/// Rewrite `tc.draw` / `tc.target` calls in a rule or invariant body, the
/// way `#[hegel::test]` does for a test body. Each invocation runs on its
/// own naming scope, so repeatability is judged within the body alone even
/// though the body runs once per step.
fn rewrite_method_draws(method: &mut syn::ImplItemFn) {
    let tc_ident = match method.sig.inputs.iter().nth(1) {
        Some(syn::FnArg::Typed(arg)) => extract_ident_from_pat(&arg.pat),
        _ => None,
    };
    if let Some(tc_ident) = tc_ident {
        rewrite_draws_in_block(&mut method.block, &tc_ident);
    }
}

struct MethodInfo {
    name: syn::Ident,
    attrs: Vec<Attribute>,
    always_run: bool,
}

fn method_entries(methods: &[MethodInfo], invariants: bool) -> Vec<TokenStream> {
    methods
        .iter()
        .map(|m| {
            let name_str = m.name.to_string();
            let name = &m.name;
            // Only forward attributes that are valid on expressions, which is a subset of all
            // attributes. See https://github.com/hegeldev/hegel-rust/pull/353.
            const FORWARDED: [&str; 7] = [
                "cfg", "cfg_attr", "allow", "expect", "warn", "deny", "forbid",
            ];
            let attrs: Vec<&Attribute> = m
                .attrs
                .iter()
                .filter(|a| FORWARDED.iter().any(|name| a.path().is_ident(name)))
                .collect();
            let constructor = match (invariants, m.always_run) {
                (false, _) => quote! { ::hegel::stateful::Rule::new },
                (true, false) => quote! { ::hegel::stateful::Invariant::new },
                (true, true) => quote! { ::hegel::stateful::Invariant::new_always_run },
            };
            // Register through a non-capturing closure rather than
            // `Self::#name` directly: `Rule.apply` is `fn(&mut M, TestCase)`,
            // and the method-call syntax inside the closure lets methods take
            // either `&self` or `&mut self` (an `&mut M` auto-coerces to
            // `&M`), as the `stateful` module docs promise for invariants.
            quote! {
                #(#attrs)*
                #constructor(
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

/// Extract the `always_run` argument of an `#[invariant]` attribute, if any.
fn invariant_always_run(attr: &Attribute) -> syn::Result<bool> {
    if matches!(attr.meta, syn::Meta::Path(_)) {
        return Ok(false);
    }
    let mut always_run = false;
    attr.parse_nested_meta(|meta| {
        if meta.path.is_ident("always_run") {
            always_run = true;
            Ok(())
        } else {
            Err(meta.error("unsupported #[invariant] argument; expected `always_run`"))
        }
    })?;
    if always_run {
        Ok(true)
    } else {
        Err(syn::Error::new_spanned(
            attr,
            "#[invariant(...)] requires `always_run`",
        ))
    }
}

struct ConcurrentMethodInfo {
    name: syn::Ident,
    group: Option<RuleGroup>,
    attrs: Vec<Attribute>,
    always_run: bool,
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
                None => {
                    let constructor = if m.always_run {
                        quote! { ::hegel::stateful::ConcurrentInvariant::new_always_run }
                    } else {
                        quote! { ::hegel::stateful::ConcurrentInvariant::new }
                    };
                    quote! {
                        #(#attrs)*
                        #constructor(
                            #name_str,
                            |__hegel_machine: &Self, __hegel_tc: ::hegel::TestCase| {
                                __hegel_machine.#name(__hegel_tc)
                            },
                        )
                    }
                }
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
            let invariant_attr = method.attrs.iter().find(|a| is_invariant(a)).cloned();
            method.attrs.retain(|a| !is_rule(a) && !is_invariant(a));

            if rule_attr.is_some() || invariant_attr.is_some() {
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

            if rule_attr.is_some() || invariant_attr.is_some() {
                rewrite_method_draws(method);
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
                    always_run: false,
                });
            }
            if let Some(attr) = invariant_attr {
                let always_run = match invariant_always_run(&attr) {
                    Ok(always_run) => always_run,
                    Err(e) => return e.to_compile_error(),
                };
                invariants.push(ConcurrentMethodInfo {
                    name: method.sig.ident.clone(),
                    group: None,
                    attrs: method.attrs.clone(),
                    always_run,
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
            let invariant_attr = method.attrs.iter().find(|a| is_invariant(a)).cloned();
            let has_invariant = invariant_attr.is_some();
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

            if has_rule || has_invariant {
                rewrite_method_draws(method);
            }

            let info = |always_run| MethodInfo {
                name: method.sig.ident.clone(),
                attrs: method.attrs.clone(),
                always_run,
            };
            if has_rule {
                rules.push(info(false));
            }
            if let Some(attr) = invariant_attr {
                let always_run = match invariant_always_run(&attr) {
                    Ok(always_run) => always_run,
                    Err(e) => return e.to_compile_error(),
                };
                invariants.push(info(always_run));
            }
        }
    }

    let block_type = &block.self_ty;
    let (impl_generics, _, where_clause) = block.generics.split_for_impl();
    let rule_entries = method_entries(&rules, false);
    let invariant_entries = method_entries(&invariants, true);

    quote! {
        #block
        impl #impl_generics ::hegel::stateful::StateMachine for #block_type #where_clause {
            fn rules(&self) -> ::std::vec::Vec<::hegel::stateful::Rule<Self>> {
                ::std::vec![ #( #rule_entries ),* ]
            }
            fn invariants(&self) -> ::std::vec::Vec<::hegel::stateful::Invariant<Self>> {
                ::std::vec![ #( #invariant_entries ),* ]
            }
        }
    }
}
