//! Shared helpers used by `#[hegel::test]`, `#[hegel::main]`, and
//! `#[hegel::standalone_function]`.
//!
//! All three macros expand to essentially the same core: rewrite
//! `tc.draw(gen)` calls into `tc.__draw_named(gen, "name", repeatable)`,
//! pick up any `#[hegel::explicit_test_case]` sibling attributes, build
//! a `Settings` expression, and then call `Hegel::new(...).settings(...).run()`.
//! They differ only in the wrapping function that holds the call.

use std::collections::HashMap;

use proc_macro2::TokenStream;
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::visit_mut::VisitMut;
use syn::{Expr, Ident, Pat, Token};

/// A single named argument in a `#[hegel::test(...)]`-style attribute.
pub struct SettingArg {
    pub key: Ident,
    pub value: Expr,
}

/// Parsed contents of a `#[hegel::test(...)]`-style attribute.
///
/// Acceptable formats:
/// - empty
/// - `settings_expr`
/// - `settings_expr, seed = 42`
/// - `seed = 42, test_cases = 10`
pub struct SettingsAttrArgs {
    pub settings: Option<Expr>,
    pub settings_args: Vec<SettingArg>,
}

impl Parse for SettingsAttrArgs {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut settings = None;
        let mut settings_args = Vec::new();

        if input.is_empty() {
            return Ok(SettingsAttrArgs {
                settings,
                settings_args,
            });
        }

        let is_named_arg = input.peek(Ident) && input.peek2(Token![=]);
        if !is_named_arg {
            settings = Some(input.parse::<Expr>()?);
            if !input.is_empty() {
                let _comma: Token![,] = input.parse()?;
            }
        }

        while !input.is_empty() {
            let key: Ident = input.parse()?;
            let _eq: Token![=] = input.parse()?;
            let value: Expr = input.parse()?;
            settings_args.push(SettingArg { key, value });
            if !input.is_empty() {
                let _comma: Token![,] = input.parse()?;
            }
        }

        Ok(SettingsAttrArgs {
            settings,
            settings_args,
        })
    }
}

impl SettingsAttrArgs {
    /// Build a token stream that evaluates to a `hegel::Settings` value.
    pub fn to_settings_expr(&self) -> TokenStream {
        let chain: Vec<TokenStream> = self
            .settings_args
            .iter()
            .map(|arg| {
                let key = &arg.key;
                let value = &arg.value;
                quote! { .#key(#value) }
            })
            .collect();

        match &self.settings {
            Some(expr) => quote! { #expr #(#chain)* },
            None if chain.is_empty() => quote! { hegel::Settings::new() },
            None => quote! { hegel::Settings::new() #(#chain)* },
        }
    }
}

/// Extract a simple identifier from a pattern, handling type annotations.
pub fn extract_ident_from_pat(pat: &Pat) -> Option<String> {
    match pat {
        Pat::Ident(pat_ident) => Some(pat_ident.ident.to_string()),
        Pat::Type(pat_type) => extract_ident_from_pat(&pat_type.pat),
        _ => None,
    }
}

/// Check if a `let` binding is of the form `let <ident> = <test_case_ident>.draw(<one_arg>)`.
fn is_test_case_draw_binding(node: &syn::Local, test_case_ident: &str) -> Option<String> {
    let var_name = extract_ident_from_pat(&node.pat)?;

    let init = node.init.as_ref()?;
    let method_call = match &*init.expr {
        Expr::MethodCall(mc) => mc,
        _ => return None,
    };

    if method_call.method != "draw" || method_call.args.len() != 1 {
        return None;
    }

    let is_tc = match &*method_call.receiver {
        Expr::Path(path) => path.path.is_ident(test_case_ident),
        _ => false,
    };
    if !is_tc {
        return None;
    }

    Some(var_name)
}

struct DrawNameCollector {
    test_case_ident: String,
    block_depth: usize,
    name_flags: HashMap<String, bool>,
}

impl VisitMut for DrawNameCollector {
    fn visit_block_mut(&mut self, node: &mut syn::Block) {
        self.block_depth += 1;
        syn::visit_mut::visit_block_mut(self, node);
        self.block_depth -= 1;
    }

    fn visit_expr_closure_mut(&mut self, node: &mut syn::ExprClosure) {
        self.block_depth += 1;
        syn::visit_mut::visit_expr_closure_mut(self, node);
        self.block_depth -= 1;
    }

    fn visit_item_fn_mut(&mut self, _node: &mut syn::ItemFn) {}

    fn visit_local_mut(&mut self, node: &mut syn::Local) {
        syn::visit_mut::visit_local_mut(self, node);

        if let Some(var_name) = is_test_case_draw_binding(node, &self.test_case_ident) {
            let repeatable = self.block_depth > 0 || self.name_flags.contains_key(&var_name);
            let entry = self.name_flags.entry(var_name).or_insert(false);
            if repeatable {
                *entry = true;
            }
        }
    }
}

struct DrawRewriter {
    test_case_ident: String,
    name_flags: HashMap<String, bool>,
}

impl VisitMut for DrawRewriter {
    fn visit_item_fn_mut(&mut self, _node: &mut syn::ItemFn) {}

    fn visit_local_mut(&mut self, node: &mut syn::Local) {
        syn::visit_mut::visit_local_mut(self, node);

        let var_name = match is_test_case_draw_binding(node, &self.test_case_ident) {
            Some(name) => name,
            None => return,
        };

        let repeatable = self.name_flags.get(&var_name).copied().unwrap_or(false);

        let init = node.init.as_mut().unwrap();
        let method_call = match &mut *init.expr {
            Expr::MethodCall(mc) => mc,
            _ => unreachable!(),
        };

        let span = method_call.method.span();
        method_call.method = Ident::new("__draw_named", span);
        method_call.args.push(Expr::Lit(syn::ExprLit {
            attrs: vec![],
            lit: syn::Lit::Str(syn::LitStr::new(&var_name, span)),
        }));
        method_call.args.push(Expr::Lit(syn::ExprLit {
            attrs: vec![],
            lit: syn::Lit::Bool(syn::LitBool::new(repeatable, span)),
        }));
    }
}

/// Rewrite `let x = <test_case_ident>.draw(gen)` into
/// `let x = <test_case_ident>.__draw_named(gen, "x", repeatable)` throughout a block.
///
/// Uses a two-pass approach so that if a name is used in a repeatable context
/// anywhere (nested block or closure), every use of that name becomes repeatable.
pub fn rewrite_draws_in_block(body: &mut syn::Block, test_case_ident: &str) {
    let mut collector = DrawNameCollector {
        test_case_ident: test_case_ident.to_string(),
        block_depth: 0,
        name_flags: HashMap::new(),
    };
    for stmt in &mut body.stmts {
        collector.visit_stmt_mut(stmt);
    }

    let mut rewriter = DrawRewriter {
        test_case_ident: test_case_ident.to_string(),
        name_flags: collector.name_flags,
    };
    for stmt in &mut body.stmts {
        rewriter.visit_stmt_mut(stmt);
    }
}

/// A parsed explicit test case: a list of (name, expression_source) pairs.
pub struct ParsedExplicitTestCase {
    pub entries: Vec<(String, String)>,
}

fn is_explicit_test_case_attr(attr: &syn::Attribute) -> bool {
    let segments: Vec<_> = attr.path().segments.iter().collect();
    segments.len() == 2 && segments[0].ident == "hegel" && segments[1].ident == "explicit_test_case"
}

/// Parsed arguments for a single `#[hegel::explicit_test_case(name = expr, ...)]`.
struct ExplicitTestCaseAttrArgs {
    entries: Vec<ExplicitTestCaseEntry>,
}

struct ExplicitTestCaseEntry {
    name: Ident,
    value: Expr,
}

impl syn::parse::Parse for ExplicitTestCaseAttrArgs {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let mut entries = Vec::new();
        while !input.is_empty() {
            let name: Ident = input.parse()?;
            let _eq: Token![=] = input.parse()?;
            let value: Expr = input.parse()?;
            entries.push(ExplicitTestCaseEntry { name, value });
            if !input.is_empty() {
                let _comma: Token![,] = input.parse()?;
            }
        }
        Ok(ExplicitTestCaseAttrArgs { entries })
    }
}

/// Extract `#[hegel::explicit_test_case(...)]` attributes from the attribute list,
/// removing them in-place. Returns `Err` with a compile error if any attribute is
/// malformed.
pub fn extract_explicit_test_cases(
    attrs: &mut Vec<syn::Attribute>,
) -> Result<Vec<ParsedExplicitTestCase>, TokenStream> {
    let mut cases = Vec::new();
    let mut error = None;
    attrs.retain(|attr| {
        if !is_explicit_test_case_attr(attr) {
            return true;
        }

        let syn::Meta::List(list) = &attr.meta else {
            error = Some(
                syn::Error::new_spanned(
                    attr,
                    "#[hegel::explicit_test_case] requires arguments.\n\
                     Usage: #[hegel::explicit_test_case(name = value, ...)]",
                )
                .to_compile_error(),
            );
            return false;
        };

        let parsed: syn::Result<ExplicitTestCaseAttrArgs> = syn::parse2(list.tokens.clone());
        match parsed {
            Ok(args) if args.entries.is_empty() => {
                error = Some(
                    syn::Error::new_spanned(
                        attr,
                        "#[hegel::explicit_test_case] requires at least one name = value pair.\n\
                         Usage: #[hegel::explicit_test_case(name = value, ...)]",
                    )
                    .to_compile_error(),
                );
            }
            Ok(args) => {
                let entries = args
                    .entries
                    .iter()
                    .map(|arg| {
                        let name = arg.name.to_string();
                        let expr = &arg.value;
                        let expr_source = quote::quote!(#expr).to_string();
                        (name, expr_source)
                    })
                    .collect();
                cases.push(ParsedExplicitTestCase { entries });
            }
            Err(e) => {
                error = Some(e.to_compile_error());
            }
        }
        false
    });
    if let Some(err) = error {
        return Err(err);
    }
    Ok(cases)
}

/// Generate a sequence of explicit test case blocks. Each block constructs an
/// `ExplicitTestCase` populated with `with_value` calls and then runs the body
/// via `ExplicitTestCase::run`.
pub fn build_explicit_blocks(
    cases: &[ParsedExplicitTestCase],
    param_pat: &Pat,
    body: &syn::Block,
) -> Vec<TokenStream> {
    cases
        .iter()
        .map(|case| {
            let with_value_calls: Vec<TokenStream> = case
                .entries
                .iter()
                .map(|(name, expr_source)| {
                    let expr: syn::Expr = syn::parse_str(expr_source).unwrap_or_else(|e| {
                        panic!("Failed to parse explicit_test_case expression: {}", e)
                    });
                    let source_lit = syn::LitStr::new(expr_source, proc_macro2::Span::call_site());
                    quote! {
                        .with_value(#name, #source_lit, #expr)
                    }
                })
                .collect();

            quote! {
                {
                    let __hegel_etc = hegel::ExplicitTestCase::new()
                        #(#with_value_calls)*;
                    __hegel_etc.run(|#param_pat: &hegel::ExplicitTestCase| #body);
                }
            }
        })
        .collect()
}
