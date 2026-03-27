use std::collections::HashMap;

use proc_macro2::TokenStream;
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::visit_mut::VisitMut;
use syn::{Expr, FnArg, Ident, ItemFn, Pat, Token};

/// A single named argument in a `#[hegel::test(...)]` expression.
struct SettingArg {
    key: Ident,
    value: Expr,
}

/// Parsed result of `#[hegel::test(...)]`.
///
/// Acceptable formats:
/// - `#[hegel::test]`
/// - `#[hegel::test(settings_expr)]`
/// - `#[hegel::test(settings_expr, seed = 42)]`
/// - `#[hegel::test(seed = 42)]`
struct TestArgs {
    settings: Option<Expr>,
    settings_args: Vec<SettingArg>,
}

impl Parse for TestArgs {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut settings = None;
        let mut settings_args = Vec::new();

        if input.is_empty() {
            return Ok(TestArgs {
                settings,
                settings_args,
            });
        }

        // check if the first arg is a settings expression or a named settingArg
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

        Ok(TestArgs {
            settings,
            settings_args,
        })
    }
}

/// Extract a simple identifier from a pattern, handling type annotations.
fn extract_ident_from_pat(pat: &Pat) -> Option<String> {
    match pat {
        Pat::Ident(pat_ident) => Some(pat_ident.ident.to_string()),
        Pat::Type(pat_type) => extract_ident_from_pat(&pat_type.pat),
        _ => None,
    }
}

/// Check if a `let` binding is of the form `let <ident> = <tc_ident>.draw(<one_arg>)`.
fn is_tc_draw_binding<'a>(node: &'a syn::Local, tc_ident: &str) -> Option<String> {
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
        Expr::Path(path) => path.path.is_ident(tc_ident),
        _ => false,
    };
    if !is_tc {
        return None;
    }

    Some(var_name)
}

/// Pass 1: Collect all draw variable names and determine per-name repeatable flags.
///
/// If any use of a name appears in a repeatable context (nested block, closure),
/// ALL uses of that name become repeatable. This ensures the runtime never sees
/// inconsistent repeatable flags for the same name.
struct DrawNameCollector {
    tc_ident: String,
    repeatable_depth: usize,
    name_flags: HashMap<String, bool>,
}

impl VisitMut for DrawNameCollector {
    fn visit_block_mut(&mut self, node: &mut syn::Block) {
        self.repeatable_depth += 1;
        syn::visit_mut::visit_block_mut(self, node);
        self.repeatable_depth -= 1;
    }

    fn visit_expr_closure_mut(&mut self, node: &mut syn::ExprClosure) {
        self.repeatable_depth += 1;
        syn::visit_mut::visit_expr_closure_mut(self, node);
        self.repeatable_depth -= 1;
    }

    fn visit_item_fn_mut(&mut self, _node: &mut syn::ItemFn) {}

    fn visit_local_mut(&mut self, node: &mut syn::Local) {
        syn::visit_mut::visit_local_mut(self, node);

        if let Some(var_name) = is_tc_draw_binding(node, &self.tc_ident) {
            let repeatable = self.repeatable_depth > 0;
            let entry = self.name_flags.entry(var_name).or_insert(false);
            if repeatable {
                *entry = true;
            }
        }
    }
}

/// Pass 2: Rewrite `let x = tc.draw(gen)` to `let x = tc.draw_named(gen, "x", repeatable)`.
///
/// Uses the pre-computed name_flags from DrawNameCollector so that every use of
/// a given name gets the same repeatable flag.
struct DrawRewriter {
    tc_ident: String,
    name_flags: HashMap<String, bool>,
}

impl VisitMut for DrawRewriter {
    fn visit_item_fn_mut(&mut self, _node: &mut syn::ItemFn) {}

    fn visit_local_mut(&mut self, node: &mut syn::Local) {
        syn::visit_mut::visit_local_mut(self, node);

        let var_name = match is_tc_draw_binding(node, &self.tc_ident) {
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
        method_call.method = Ident::new("draw_named", span);
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

pub fn expand_test(attr: proc_macro2::TokenStream, item: proc_macro2::TokenStream) -> TokenStream {
    let test_args: TestArgs = if attr.is_empty() {
        TestArgs {
            settings: None,
            settings_args: Vec::new(),
        }
    } else {
        match syn::parse2(attr) {
            Ok(args) => args,
            Err(e) => return e.to_compile_error(),
        }
    };

    let func: ItemFn = match syn::parse2(item) {
        Ok(f) => f,
        Err(e) => return e.to_compile_error(),
    };

    if func.sig.inputs.len() != 1 {
        return syn::Error::new_spanned(
            &func.sig,
            "#[hegel::test] functions must take exactly one parameter of type hegel::TestCase.",
        )
        .to_compile_error();
    }

    let param = &func.sig.inputs[0];
    let param_typed = match param {
        FnArg::Typed(pat_type) => pat_type,
        FnArg::Receiver(_) => {
            return syn::Error::new_spanned(
                param,
                "#[hegel::test] functions cannot have a self parameter.",
            )
            .to_compile_error();
        }
    };
    let param_pat = &param_typed.pat;
    let param_ty = &param_typed.ty;

    for attr in &func.attrs {
        if attr.path().is_ident("test") {
            return syn::Error::new_spanned(
                attr,
                "#[hegel::test] used on a function with #[test].\
                Remove the #[test] attribute; [hegel::test] automatically adds #[test].",
            )
            .to_compile_error();
        }
    }

    // Rewrite `let x = tc.draw(gen)` -> `let x = tc.draw_named(gen, "x", repeatable)`
    //
    // Two-pass approach:
    //   1. Collect all draw variable names and determine per-name repeatable flags.
    //      If any use of a name is in a nested block/closure, all uses are repeatable.
    //   2. Rewrite draws using the computed flags.
    //
    // We visit the function body's statements directly (not the block itself) so that
    // the outermost block doesn't count as a nesting level.
    let body = {
        let mut body = (*func.block).clone();
        if let Some(tc_name) = extract_ident_from_pat(param_pat) {
            // Pass 1: collect names
            let mut collector = DrawNameCollector {
                tc_ident: tc_name.clone(),
                repeatable_depth: 0,
                name_flags: HashMap::new(),
            };
            for stmt in &mut body.stmts {
                collector.visit_stmt_mut(stmt);
            }

            // Pass 2: rewrite
            let mut rewriter = DrawRewriter {
                tc_ident: tc_name,
                name_flags: collector.name_flags,
            };
            for stmt in &mut body.stmts {
                rewriter.visit_stmt_mut(stmt);
            }
        }
        body
    };

    let test_name = func.sig.ident.to_string();

    let settings_args_chain: Vec<TokenStream> = test_args
        .settings_args
        .iter()
        .map(|arg| {
            let key = &arg.key;
            let value = &arg.value;
            quote! { .#key(#value) }
        })
        .collect();

    let settings_expr = match &test_args.settings {
        Some(expr) => quote! { #expr #(#settings_args_chain)* },
        None if settings_args_chain.is_empty() => quote! { hegel::Settings::new() },
        None => quote! { hegel::Settings::new() #(#settings_args_chain)* },
    };

    let new_body: TokenStream = quote! {
        {
            hegel::Hegel::new(|#param_pat: #param_ty| #body)
            .settings(#settings_expr)
            .__database_key(format!("{}::{}", module_path!(), #test_name))
            .test_location(hegel::TestLocation {
                function: #test_name.to_string(),
                file: file!().to_string(),
                class: module_path!().to_string(),
                begin_line: line!(),
            })
            .run();
        }
    };

    let new_block: syn::Block = syn::parse2(new_body).unwrap();

    let mut func = func;
    func.sig.inputs.clear();
    *func.block = new_block;

    quote! {
        #[test]
        #func
    }
}
