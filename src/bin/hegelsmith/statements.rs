use crate::gen_expr::{self, GenExpr};
use crate::types::{self, RustType};
use hegel::TestCase;
use hegel::generators;

/// A variable binding in scope.
#[derive(Debug, Clone)]
pub struct VarInfo {
    pub name: String,
    pub rust_type: RustType,
}

/// A statement in the generated test body.
#[derive(Debug, Clone)]
pub enum Statement {
    Draw {
        var_name: String,
        type_annotation: RustType,
        gen_expr: GenExpr,
    },
    Assert {
        condition: String,
    },
    AssertEq {
        left: String,
        right: String,
    },
    Assume {
        condition: String,
    },
    Note {
        message: String,
    },
    DependentDraw {
        var_name: String,
        type_annotation: RustType,
        gen_source: String,
    },
    IfBlock {
        condition: String,
        then_body: Vec<Statement>,
        else_body: Option<Vec<Statement>>,
    },
    Target {
        score_expr: String,
        label: String,
    },
}

impl Statement {
    /// Render this statement as Rust source code lines (without leading indentation).
    pub fn render(&self) -> String {
        match self {
            Statement::Draw {
                var_name,
                type_annotation,
                gen_expr,
            } => {
                format!(
                    "let {var_name}: {} = tc.draw({});",
                    type_annotation.render(),
                    gen_expr.render()
                )
            }
            Statement::Assert { condition } => format!("assert!({condition});"),
            Statement::AssertEq { left, right } => format!("assert_eq!({left}, {right});"),
            Statement::Assume { condition } => format!("tc.assume({condition});"),
            Statement::Note { message } => format!("tc.note(&format!({message}));"),
            Statement::DependentDraw {
                var_name,
                type_annotation,
                gen_source,
            } => {
                format!(
                    "let {var_name}: {} = tc.draw({gen_source});",
                    type_annotation.render()
                )
            }
            Statement::Target { score_expr, label } => {
                format!("tc.target({score_expr}, \"{label}\");")
            }
            Statement::IfBlock {
                condition,
                then_body,
                else_body,
            } => {
                let mut s = format!("if {condition} {{\n");
                for stmt in then_body {
                    for line in stmt.render().lines() {
                        s.push_str(&format!("    {line}\n"));
                    }
                }
                if let Some(else_stmts) = else_body {
                    s.push_str("} else {\n");
                    for stmt in else_stmts {
                        for line in stmt.render().lines() {
                            s.push_str(&format!("    {line}\n"));
                        }
                    }
                }
                s.push('}');
                s
            }
        }
    }
}

/// Track the environment of variables in scope.
pub struct Env {
    pub vars: Vec<VarInfo>,
    next_id: usize,
}

impl Default for Env {
    fn default() -> Self {
        Self::new()
    }
}

impl Env {
    pub fn new() -> Self {
        Env {
            vars: Vec::new(),
            next_id: 0,
        }
    }

    pub fn fresh_var(&mut self) -> String {
        let name = format!("v{}", self.next_id);
        self.next_id += 1;
        name
    }

    pub fn add_var(&mut self, name: String, rust_type: RustType) {
        self.vars.push(VarInfo { name, rust_type });
    }

    pub fn vars_of_type<F: Fn(&RustType) -> bool>(&self, pred: F) -> Vec<&VarInfo> {
        self.vars.iter().filter(|v| pred(&v.rust_type)).collect()
    }

    pub fn integer_vars(&self) -> Vec<&VarInfo> {
        self.vars_of_type(|t| t.is_integer())
    }

    pub fn collection_vars(&self) -> Vec<&VarInfo> {
        self.vars_of_type(|t| t.is_collection())
    }

    pub fn string_vars(&self) -> Vec<&VarInfo> {
        self.vars_of_type(|t| t.is_string_like())
    }

    pub fn bool_vars(&self) -> Vec<&VarInfo> {
        self.vars_of_type(|t| *t == RustType::Bool)
    }

    /// Pick a random variable from this env.
    fn pick_var<'a>(&'a self, tc: &TestCase) -> &'a VarInfo {
        let idx: usize = tc.draw(
            generators::integers::<usize>()
                .min_value(0)
                .max_value(self.vars.len() - 1),
        );
        &self.vars[idx]
    }

    /// Pick a random variable from a filtered set.
    fn pick_from<'a>(tc: &TestCase, vars: &[&'a VarInfo]) -> &'a VarInfo {
        let idx: usize = tc.draw(
            generators::integers::<usize>()
                .min_value(0)
                .max_value(vars.len() - 1),
        );
        vars[idx]
    }

    /// Find two integer vars (possibly the same).
    fn two_integer_vars(&self, tc: &TestCase) -> Option<(&VarInfo, &VarInfo)> {
        let ivars = self.integer_vars();
        if ivars.is_empty() {
            return None;
        }
        let a = Self::pick_from(tc, &ivars);
        let b = Self::pick_from(tc, &ivars);
        // Both must be the same type for arithmetic
        if a.rust_type == b.rust_type {
            Some((a, b))
        } else {
            Some((a, a))
        }
    }

    /// Save/restore for block scoping: returns the current length.
    fn save(&self) -> usize {
        self.vars.len()
    }

    /// Remove variables added since save point.
    fn restore(&mut self, save_point: usize) {
        self.vars.truncate(save_point);
    }
}

// ---- Top-level statement generation ----

/// Generate a sequence of statements that form the test body.
/// Guarantees at least one assertion, and usually 2-3.
pub fn generate_statements(tc: &TestCase) -> Vec<Statement> {
    let mut env = Env::new();
    let mut stmts = Vec::new();

    // Phase 1: Generate 1-5 initial draws
    let num_draws: usize = tc.draw(generators::integers::<usize>().min_value(1).max_value(5));
    for _ in 0..num_draws {
        let stmt = gen_draw_or_dependent(tc, &mut env);
        stmts.push(stmt);
    }

    // Phase 2: Generate 1-3 assertion blocks, interspersed with optional extra statements
    let num_assertion_blocks: usize =
        tc.draw(generators::integers::<usize>().min_value(1).max_value(3));
    for i in 0..num_assertion_blocks {
        // Sometimes add a draw or note before the assertion
        if i > 0 && tc.draw(generators::booleans()) {
            stmts.push(gen_misc_statement(tc, &mut env));
        }

        // Generate an assertion block: 1-2 assertions, possibly compound
        let num_asserts: usize = tc.draw(generators::integers::<usize>().min_value(1).max_value(2));
        for _ in 0..num_asserts {
            stmts.push(gen_rich_assertion(tc, &env));
        }
    }

    // Phase 3: Optionally add an if block
    if tc.draw(generators::booleans()) && !env.vars.is_empty() {
        stmts.push(gen_if_block(tc, &mut env, 0));
    }

    // Phase 4: Optionally add 0-3 tc.target() calls. These populate the
    // native backend's pareto front / targeted-optimiser state, which
    // would otherwise never be exercised.
    let has_numeric = env
        .vars
        .iter()
        .any(|v| v.rust_type.is_integer() || v.rust_type.is_float());
    if has_numeric {
        // Bias toward at least one target call so the pareto / targeted
        // optimiser paths actually run; hegel's exploration heavily
        // oversamples the lower bound, so min=0 leaves them unexercised.
        let num_targets: usize = tc.draw(generators::integers::<usize>().min_value(1).max_value(3));
        for _ in 0..num_targets {
            stmts.push(gen_target_statement(tc, &env));
        }
    }

    // Phase 5: Optionally add trailing draws/notes/assumes
    let num_trailing: usize = tc.draw(generators::integers::<usize>().min_value(0).max_value(2));
    for _ in 0..num_trailing {
        stmts.push(gen_misc_statement(tc, &mut env));
    }

    stmts
}

/// Generate a draw or dependent draw.
fn gen_draw_or_dependent(tc: &TestCase, env: &mut Env) -> Statement {
    let has_int_vars = !env.integer_vars().is_empty();
    let choice: u8 = if has_int_vars {
        tc.draw(generators::integers::<u8>().min_value(0).max_value(9))
    } else {
        0
    };
    match choice {
        0..=5 => gen_draw_statement(tc, env, 2),
        6..=8 => gen_dependent_draw(tc, env),
        9 => gen_dependent_compose(tc, env),
        _ => unreachable!(),
    }
}

/// Generate a misc statement (draw, note, assume, target).
fn gen_misc_statement(tc: &TestCase, env: &mut Env) -> Statement {
    let has_vars = !env.vars.is_empty();
    let has_numeric = env
        .vars
        .iter()
        .any(|v| v.rust_type.is_integer() || v.rust_type.is_float());
    let choice: u8 = if has_vars {
        tc.draw(generators::integers::<u8>().min_value(0).max_value(6))
    } else {
        0
    };
    match choice {
        0..=2 => gen_draw_or_dependent(tc, env),
        3 => gen_note_statement(tc, env),
        4 => gen_assume_statement(tc, env),
        5 => gen_draw_or_dependent(tc, env),
        6 => {
            if has_numeric {
                gen_target_statement(tc, env)
            } else {
                gen_draw_or_dependent(tc, env)
            }
        }
        _ => unreachable!(),
    }
}

fn gen_draw_statement(tc: &TestCase, env: &mut Env, depth: usize) -> Statement {
    let choice: u8 = tc.draw(generators::integers::<u8>().min_value(0).max_value(5));
    match choice {
        0..=3 => {
            let rt = types::gen_type(tc, 1);
            let ge = gen_expr::gen_expr_for_type(tc, &rt, depth);
            let var = env.fresh_var();
            let name = var.clone();
            env.add_var(var, rt.clone());
            Statement::Draw {
                var_name: name,
                type_annotation: rt,
                gen_expr: ge,
            }
        }
        4 => {
            let ge = gen_expr::gen_compose_expr(tc, depth);
            let rt = ge.output_type();
            let var = env.fresh_var();
            let name = var.clone();
            env.add_var(var, rt.clone());
            Statement::Draw {
                var_name: name,
                type_annotation: rt,
                gen_expr: ge,
            }
        }
        5 => {
            let ge = gen_expr::gen_flat_map_expr(tc);
            let rt = ge.output_type();
            let var = env.fresh_var();
            let name = var.clone();
            env.add_var(var, rt.clone());
            Statement::Draw {
                var_name: name,
                type_annotation: rt,
                gen_expr: ge,
            }
        }
        _ => unreachable!(),
    }
}

// ---- Rich assertion generation ----

/// Generate a non-trivial assertion. May be:
/// - A single atomic predicate
/// - A multi-value predicate (involving 2+ variables)
/// - A compound predicate (boolean combinations)
/// - A computed-value predicate (sums, element access, etc.)
fn gen_rich_assertion(tc: &TestCase, env: &Env) -> Statement {
    if env.vars.is_empty() {
        return Statement::Assert {
            condition: "false".into(),
        };
    }

    let choice: u8 = tc.draw(generators::integers::<u8>().min_value(0).max_value(4));
    match choice {
        // Single atomic predicate
        0 => Statement::Assert {
            condition: gen_atomic_predicate(tc, env),
        },
        // Multi-value predicate
        1 => Statement::Assert {
            condition: gen_multi_value_predicate(tc, env),
        },
        // Compound predicate (boolean combination)
        2 => Statement::Assert {
            condition: gen_compound_predicate(tc, env, 0),
        },
        // Computed value predicate
        3 => Statement::Assert {
            condition: gen_computed_predicate(tc, env),
        },
        // assert_eq with computed values
        4 => gen_assert_eq_computed(tc, env),
        _ => unreachable!(),
    }
}

// ---- Atomic predicates (single variable) ----

fn gen_atomic_predicate(tc: &TestCase, env: &Env) -> String {
    let var = env.pick_var(tc);
    gen_predicate_for_var(tc, var)
}

fn gen_predicate_for_var(tc: &TestCase, var: &VarInfo) -> String {
    let name = &var.name;
    match &var.rust_type {
        t if t.is_integer() => gen_int_predicate(tc, name, t),
        t if t.is_float() => gen_float_predicate(tc, name, t),
        RustType::Bool => gen_bool_predicate(tc, name),
        RustType::String => gen_string_predicate(tc, name),
        RustType::Vec(_) | RustType::VecU8 | RustType::HashSet(_) | RustType::HashMap(_, _) => {
            gen_collection_predicate(tc, name)
        }
        RustType::Option(_) => gen_option_predicate(tc, name),
        _ => format!("{name} == {name}"),
    }
}

fn gen_int_predicate(tc: &TestCase, name: &str, rt: &RustType) -> String {
    let type_name = rt.render();
    let is_signed = rt.is_signed_int();
    let choice: u8 = tc.draw(generators::integers::<u8>().min_value(0).max_value(6));
    match choice {
        0 => format!("{name} > 0"),
        1 => {
            let t: i64 = if is_signed {
                tc.draw(generators::integers::<i64>().min_value(-100).max_value(100))
            } else {
                tc.draw(generators::integers::<i64>().min_value(0).max_value(200))
            };
            format!("{name} < {t}_{type_name}")
        }
        2 => {
            let n: u8 = tc.draw(generators::integers::<u8>().min_value(2).max_value(10));
            format!("{name} % {n}_{type_name} == 0")
        }
        3 => {
            if is_signed {
                format!("{name} >= 0")
            } else {
                format!("{name} < 100_{type_name}")
            }
        }
        4 => {
            let val: i64 = if is_signed {
                tc.draw(generators::integers::<i64>().min_value(-10).max_value(10))
            } else {
                tc.draw(generators::integers::<i64>().min_value(0).max_value(20))
            };
            format!("{name} != {val}_{type_name}")
        }
        5 => {
            if !is_signed {
                format!("{name} == 0 || {name}.is_power_of_two()")
            } else {
                format!("{name}.abs() < 50_{type_name}")
            }
        }
        6 => format!("{name}.wrapping_mul({name}) > {name}"),
        _ => unreachable!(),
    }
}

fn gen_float_predicate(tc: &TestCase, name: &str, rt: &RustType) -> String {
    let choice: u8 = tc.draw(generators::integers::<u8>().min_value(0).max_value(4));
    match choice {
        0 => format!("{name} > 0.0"),
        1 => format!("{name}.is_finite()"),
        2 => format!("{name} == {name}"),
        3 => {
            let t: f64 = tc.draw(
                generators::floats::<f64>()
                    .min_value(-100.0)
                    .max_value(100.0),
            );
            format!("{name} < {t}_{}", rt.render())
        }
        4 => format!("{name}.abs() < 1.0"),
        _ => unreachable!(),
    }
}

fn gen_bool_predicate(tc: &TestCase, name: &str) -> String {
    if tc.draw(generators::booleans()) {
        name.to_string()
    } else {
        format!("!{name}")
    }
}

fn gen_string_predicate(tc: &TestCase, name: &str) -> String {
    let choice: u8 = tc.draw(generators::integers::<u8>().min_value(0).max_value(5));
    match choice {
        0 => format!("{name}.is_empty()"),
        1 => format!("!{name}.is_empty()"),
        2 => {
            let t: usize = tc.draw(generators::integers::<usize>().min_value(1).max_value(20));
            format!("{name}.len() < {t}")
        }
        3 => {
            let c: char = tc.draw(generators::sampled_from(vec![
                'a', 'e', 'i', 'o', 'u', ' ', '0', 'A',
            ]));
            format!("{name}.contains('{c}')")
        }
        4 => format!("{name}.is_ascii()"),
        5 => {
            let prefix = tc.draw(generators::sampled_from(vec![
                "a".to_string(),
                "the".to_string(),
                "http".to_string(),
                "0".to_string(),
            ]));
            format!("{name}.starts_with(\"{prefix}\")")
        }
        _ => unreachable!(),
    }
}

fn gen_collection_predicate(tc: &TestCase, name: &str) -> String {
    let choice: u8 = tc.draw(generators::integers::<u8>().min_value(0).max_value(4));
    match choice {
        0 => format!("{name}.is_empty()"),
        1 => format!("!{name}.is_empty()"),
        2 => {
            let t: usize = tc.draw(generators::integers::<usize>().min_value(1).max_value(10));
            format!("{name}.len() < {t}")
        }
        3 => {
            let t: usize = tc.draw(generators::integers::<usize>().min_value(1).max_value(5));
            format!("{name}.len() > {t}")
        }
        4 => {
            let t: usize = tc.draw(generators::integers::<usize>().min_value(0).max_value(5));
            format!("{name}.len() == {t}")
        }
        _ => unreachable!(),
    }
}

fn gen_option_predicate(tc: &TestCase, name: &str) -> String {
    if tc.draw(generators::booleans()) {
        format!("{name}.is_some()")
    } else {
        format!("{name}.is_none()")
    }
}

// ---- Multi-value predicates (involving 2+ variables) ----

fn gen_multi_value_predicate(tc: &TestCase, env: &Env) -> String {
    // Try to find two integer vars of the same type
    if let Some((a, b)) = env.two_integer_vars(tc) {
        let an = &a.name;
        let bn = &b.name;
        let tn = a.rust_type.render();
        let choice: u8 = tc.draw(generators::integers::<u8>().min_value(0).max_value(7));
        return match choice {
            0 => format!("{an}.wrapping_add({bn}) > 0_{tn}"),
            1 => format!("{an} < {bn}"),
            2 => format!("{an} == {bn}"),
            3 => format!("{an} != {bn}"),
            4 => format!("{an}.wrapping_add({bn}) == {bn}.wrapping_add({an})"),
            5 => format!("{an} > {bn} || {an} <= {bn}"),
            6 => format!("{an}.wrapping_mul({bn}) == {bn}.wrapping_mul({an})"),
            7 => format!("{an}.wrapping_sub({bn}) > 0_{tn}"),
            _ => unreachable!(),
        };
    }

    // Try two string vars
    let svars = env.string_vars();
    if svars.len() >= 2 {
        let a = Env::pick_from(tc, &svars);
        let b_candidates: Vec<_> = svars.iter().filter(|v| v.name != a.name).copied().collect();
        if !b_candidates.is_empty() {
            let b = Env::pick_from(tc, &b_candidates);
            let choice: u8 = tc.draw(generators::integers::<u8>().min_value(0).max_value(2));
            return match choice {
                0 => format!("{}.len() < {}.len()", a.name, b.name),
                1 => format!("{} == {}", a.name, b.name),
                2 => format!("{}.len() + {}.len() < 100", a.name, b.name),
                _ => unreachable!(),
            };
        }
    }

    // Try two collection vars
    let cvars = env.collection_vars();
    if cvars.len() >= 2 {
        let a = Env::pick_from(tc, &cvars);
        let b_candidates: Vec<_> = cvars.iter().filter(|v| v.name != a.name).copied().collect();
        if !b_candidates.is_empty() {
            let b = Env::pick_from(tc, &b_candidates);
            let choice: u8 = tc.draw(generators::integers::<u8>().min_value(0).max_value(1));
            return match choice {
                0 => format!("{}.len() == {}.len()", a.name, b.name),
                1 => format!("{}.len() + {}.len() < 20", a.name, b.name),
                _ => unreachable!(),
            };
        }
    }

    // Fallback: combine an int and a collection
    let ivars = env.integer_vars();
    if !ivars.is_empty() && !cvars.is_empty() {
        let iv = Env::pick_from(tc, &ivars);
        let cv = Env::pick_from(tc, &cvars);
        return format!("({} as usize) < {}.len() + 1", iv.name, cv.name);
    }

    // Final fallback: atomic
    gen_atomic_predicate(tc, env)
}

// ---- Compound predicates (boolean combinations) ----

fn gen_compound_predicate(tc: &TestCase, env: &Env, depth: usize) -> String {
    if depth >= 2 {
        return gen_atomic_predicate(tc, env);
    }

    let choice: u8 = tc.draw(generators::integers::<u8>().min_value(0).max_value(4));
    match choice {
        // p1 && p2
        0 => {
            let p1 = gen_leaf_or_compound(tc, env, depth + 1);
            let p2 = gen_leaf_or_compound(tc, env, depth + 1);
            format!("({p1}) && ({p2})")
        }
        // p1 || p2
        1 => {
            let p1 = gen_leaf_or_compound(tc, env, depth + 1);
            let p2 = gen_leaf_or_compound(tc, env, depth + 1);
            format!("({p1}) || ({p2})")
        }
        // !p
        2 => {
            let p = gen_atomic_predicate(tc, env);
            format!("!({p})")
        }
        // p1 && !p2
        3 => {
            let p1 = gen_atomic_predicate(tc, env);
            let p2 = gen_atomic_predicate(tc, env);
            format!("({p1}) && !({p2})")
        }
        // (p1 || p2) && p3
        4 => {
            let p1 = gen_atomic_predicate(tc, env);
            let p2 = gen_atomic_predicate(tc, env);
            let p3 = gen_atomic_predicate(tc, env);
            format!("(({p1}) || ({p2})) && ({p3})")
        }
        _ => unreachable!(),
    }
}

fn gen_leaf_or_compound(tc: &TestCase, env: &Env, depth: usize) -> String {
    if tc.draw(generators::booleans()) {
        gen_atomic_predicate(tc, env)
    } else {
        gen_compound_predicate(tc, env, depth)
    }
}

// ---- Computed-value predicates (sums, element access, etc.) ----

fn gen_computed_predicate(tc: &TestCase, env: &Env) -> String {
    // Try vec-of-integers for aggregate predicates
    let vec_int_vars: Vec<_> = env
        .vars
        .iter()
        .filter(|v| match &v.rust_type {
            RustType::Vec(inner) => inner.is_integer(),
            _ => false,
        })
        .collect();

    if !vec_int_vars.is_empty() {
        let var = Env::pick_from(tc, &vec_int_vars);
        let inner_type = match &var.rust_type {
            RustType::Vec(inner) => inner.render(),
            _ => unreachable!(),
        };
        let name = &var.name;
        let choice: u8 = tc.draw(generators::integers::<u8>().min_value(0).max_value(3));
        return match choice {
            // sum of elements
            0 => format!(
                "{name}.iter().copied().reduce(|a, b| a.wrapping_add(b)).unwrap_or(0_{inner_type}) > 0_{inner_type}"
            ),
            // all elements satisfy predicate
            1 => format!("{name}.iter().all(|x| *x > 0_{inner_type})"),
            // any element satisfies predicate
            2 => format!("{name}.iter().any(|x| *x == 0_{inner_type})"),
            // first element check (guarded)
            3 => format!("{name}.is_empty() || {name}[0] > 0_{inner_type}"),
            _ => unreachable!(),
        };
    }

    // Try two integer vars for arithmetic
    if let Some((a, b)) = env.two_integer_vars(tc) {
        let an = &a.name;
        let bn = &b.name;
        let choice: u8 = tc.draw(generators::integers::<u8>().min_value(0).max_value(3));
        return match choice {
            0 => format!("{an}.wrapping_add({bn}).wrapping_sub({bn}) == {an}"),
            1 => format!("{an}.wrapping_add({bn}) > {an}"),
            2 => format!("({an} as i128).wrapping_add({bn} as i128) < 1000_i128"),
            3 => format!("{an}.checked_add({bn}).is_some() || {an}.wrapping_add({bn}) < {an}"),
            _ => unreachable!(),
        };
    }

    // Try string
    let svars = env.string_vars();
    if !svars.is_empty() {
        let var = Env::pick_from(tc, &svars);
        let name = &var.name;
        let choice: u8 = tc.draw(generators::integers::<u8>().min_value(0).max_value(2));
        return match choice {
            0 => format!("{name}.chars().count() == {name}.len()"),
            1 => format!("{name}.to_uppercase().len() >= {name}.len()"),
            2 => format!("{name}.trim().len() <= {name}.len()"),
            _ => unreachable!(),
        };
    }

    // Fallback
    gen_atomic_predicate(tc, env)
}

fn gen_assert_eq_computed(tc: &TestCase, env: &Env) -> Statement {
    // Try integer arithmetic identities
    if let Some((a, b)) = env.two_integer_vars(tc) {
        let an = &a.name;
        let bn = &b.name;
        let choice: u8 = tc.draw(generators::integers::<u8>().min_value(0).max_value(2));
        return match choice {
            0 => Statement::AssertEq {
                left: format!("{an}.wrapping_add({bn})"),
                right: format!("{bn}.wrapping_add({an})"),
            },
            1 => Statement::AssertEq {
                left: format!("{an}.wrapping_mul({bn})"),
                right: format!("{bn}.wrapping_mul({an})"),
            },
            2 => Statement::AssertEq {
                left: format!("{an}.wrapping_add({bn}).wrapping_sub({bn})"),
                right: an.clone(),
            },
            _ => unreachable!(),
        };
    }

    // Try string operations
    let svars = env.string_vars();
    if !svars.is_empty() {
        let var = Env::pick_from(tc, &svars);
        let name = &var.name;
        return Statement::AssertEq {
            left: format!("{name}.to_lowercase().to_lowercase()"),
            right: format!("{name}.to_lowercase()"),
        };
    }

    // Fallback
    Statement::Assert {
        condition: gen_atomic_predicate(tc, env),
    }
}

// ---- If blocks ----

/// Generate an if block with optional else, nestable up to depth 3.
fn gen_if_block(tc: &TestCase, env: &mut Env, block_depth: usize) -> Statement {
    let condition = gen_if_condition(tc, env);

    // Generate then-body (1-3 statements)
    let then_body = gen_block_body(tc, env, block_depth);

    // Optionally generate else-body
    let else_body = if tc.draw(generators::booleans()) {
        Some(gen_block_body(tc, env, block_depth))
    } else {
        None
    };

    Statement::IfBlock {
        condition,
        then_body,
        else_body,
    }
}

/// Generate a condition expression for an if block.
fn gen_if_condition(tc: &TestCase, env: &Env) -> String {
    if env.vars.is_empty() {
        return "true".into();
    }

    let choice: u8 = tc.draw(generators::integers::<u8>().min_value(0).max_value(5));
    match choice {
        // A bool variable
        0 => {
            let bvars = env.bool_vars();
            if !bvars.is_empty() {
                let var = Env::pick_from(tc, &bvars);
                return var.name.clone();
            }
            gen_predicate_for_var(tc, env.pick_var(tc))
        }
        // Negated bool variable
        1 => {
            let bvars = env.bool_vars();
            if !bvars.is_empty() {
                let var = Env::pick_from(tc, &bvars);
                return format!("!{}", var.name);
            }
            gen_predicate_for_var(tc, env.pick_var(tc))
        }
        // Integer comparison
        2 => {
            let ivars = env.integer_vars();
            if !ivars.is_empty() {
                let var = Env::pick_from(tc, &ivars);
                return gen_int_predicate(tc, &var.name, &var.rust_type);
            }
            gen_predicate_for_var(tc, env.pick_var(tc))
        }
        // Collection non-empty check
        3 => {
            let cvars = env.collection_vars();
            if !cvars.is_empty() {
                let var = Env::pick_from(tc, &cvars);
                return format!("!{}.is_empty()", var.name);
            }
            gen_predicate_for_var(tc, env.pick_var(tc))
        }
        // Option is_some
        4 => {
            let ovars = env.vars_of_type(|t| matches!(t, RustType::Option(_)));
            if !ovars.is_empty() {
                let var = Env::pick_from(tc, &ovars);
                return format!("{}.is_some()", var.name);
            }
            gen_predicate_for_var(tc, env.pick_var(tc))
        }
        // Any predicate
        5 => gen_predicate_for_var(tc, env.pick_var(tc)),
        _ => unreachable!(),
    }
}

/// Generate the body of an if/else block.
/// Variables drawn inside are scoped to the block.
fn gen_block_body(tc: &TestCase, env: &mut Env, block_depth: usize) -> Vec<Statement> {
    let save = env.save();
    let mut stmts = Vec::new();

    let num_stmts: usize = tc.draw(generators::integers::<usize>().min_value(1).max_value(3));
    for _ in 0..num_stmts {
        let choice: u8 = tc.draw(generators::integers::<u8>().min_value(0).max_value(4));
        match choice {
            // Draw
            0 | 1 => stmts.push(gen_draw_or_dependent(tc, env)),
            // Assert
            2 | 3 => {
                if !env.vars.is_empty() {
                    stmts.push(gen_rich_assertion(tc, env));
                } else {
                    stmts.push(gen_draw_statement(tc, env, 1));
                }
            }
            // Nested if block (respect depth limit)
            4 => {
                if block_depth < 2 && !env.vars.is_empty() {
                    stmts.push(gen_if_block(tc, env, block_depth + 1));
                } else if !env.vars.is_empty() {
                    stmts.push(gen_rich_assertion(tc, env));
                } else {
                    stmts.push(gen_draw_statement(tc, env, 1));
                }
            }
            _ => unreachable!(),
        }
    }

    // Remove block-scoped variables
    env.restore(save);
    stmts
}

// ---- Dependent draws ----

fn gen_dependent_draw(tc: &TestCase, env: &mut Env) -> Statement {
    let int_vars = env.integer_vars();
    if int_vars.is_empty() {
        return gen_draw_statement(tc, env, 1);
    }

    let source_var = Env::pick_from(tc, &int_vars).clone();
    let var = env.fresh_var();
    let name = var.clone();

    let choice: u8 = tc.draw(generators::integers::<u8>().min_value(0).max_value(3));

    let (gen_source, rt) = match choice {
        0 | 1 => {
            let type_name = source_var.rust_type.render();
            let src = &source_var.name;
            let delta: u8 = tc.draw(generators::integers::<u8>().min_value(1).max_value(50));
            let gen_s = format!(
                "generators::integers::<{type_name}>().min_value({src}).max_value({src}.saturating_add({delta}_{type_name}))"
            );
            (gen_s, source_var.rust_type.clone())
        }
        2 => {
            let src = &source_var.name;
            let gen_s = format!(
                "generators::vecs(generators::booleans()).min_size(0).max_size(({src} as usize) % 10 + 1)"
            );
            (gen_s, RustType::Vec(Box::new(RustType::Bool)))
        }
        3 => {
            let src = &source_var.name;
            let gen_s =
                format!("generators::text().min_size(0).max_size(({src} as usize) % 20 + 1)");
            (gen_s, RustType::String)
        }
        _ => unreachable!(),
    };

    env.add_var(var, rt.clone());

    Statement::DependentDraw {
        var_name: name,
        type_annotation: rt,
        gen_source,
    }
}

fn gen_dependent_compose(tc: &TestCase, env: &mut Env) -> Statement {
    let int_vars = env.integer_vars();

    if int_vars.is_empty() {
        return gen_draw_statement(tc, env, 2);
    }

    let source_var = Env::pick_from(tc, &int_vars).clone();
    let src = &source_var.name;
    let src_type = source_var.rust_type.render();

    let body = format!(
        "        let bound: usize = ({src} as usize) % 10 + 1;\n        \
         let items: Vec<{src_type}> = tc.draw(generators::vecs(generators::integers::<{src_type}>()).min_size(1).max_size(bound));\n        \
         items"
    );

    let result_type = RustType::Vec(Box::new(source_var.rust_type.clone()));
    let ge = GenExpr::Compose {
        body,
        result_type: result_type.clone(),
    };

    let var = env.fresh_var();
    let name = var.clone();
    env.add_var(var, result_type.clone());

    Statement::Draw {
        var_name: name,
        type_annotation: result_type,
        gen_expr: ge,
    }
}

// ---- Notes and assumes ----

fn gen_note_statement(tc: &TestCase, env: &Env) -> Statement {
    if env.vars.is_empty() {
        return Statement::Note {
            message: "\"test running\"".into(),
        };
    }
    let var = env.pick_var(tc);
    Statement::Note {
        message: format!("\"{} = {{:?}}\", {}", var.name, var.name),
    }
}

/// Generate a `tc.target(score, label)` call. Picks a numeric variable in
/// scope and projects it to f64 with one of a few simple expressions.
/// Drives the pareto-front / targeted-optimiser code paths in the native
/// backend.
fn gen_target_statement(tc: &TestCase, env: &Env) -> Statement {
    let numeric_vars: Vec<&VarInfo> = env
        .vars
        .iter()
        .filter(|v| v.rust_type.is_integer() || v.rust_type.is_float())
        .collect();
    let var = Env::pick_from(tc, &numeric_vars);
    let name = &var.name;
    let score_expr = if var.rust_type.is_float() {
        match tc.draw(generators::integers::<u8>().min_value(0).max_value(2)) {
            0 => format!("({name} as f64)"),
            1 => format!("({name} as f64).abs()"),
            _ => format!("if ({name} as f64).is_finite() {{ {name} as f64 }} else {{ 0.0 }}"),
        }
    } else {
        // Integer: cast through i128 so we don't overflow on extreme widths,
        // then to f64 for the API.
        match tc.draw(generators::integers::<u8>().min_value(0).max_value(2)) {
            0 => format!("({name} as i128) as f64"),
            1 => format!("(({name} as i128).wrapping_neg()) as f64"),
            _ => format!("(({name} as i128).abs()) as f64"),
        }
    };
    let label = match tc.draw(generators::integers::<u8>().min_value(0).max_value(2)) {
        0 => "".to_string(),
        1 => "score".to_string(),
        _ => format!("score_{name}"),
    };
    Statement::Target { score_expr, label }
}

fn gen_assume_statement(tc: &TestCase, env: &Env) -> Statement {
    let int_vars = env.integer_vars();
    let collection_vars = env.collection_vars();
    let string_vars = env.string_vars();

    let mut options: Vec<u8> = Vec::new();
    if !int_vars.is_empty() {
        options.push(0);
    }
    if !collection_vars.is_empty() {
        options.push(1);
    }
    if !string_vars.is_empty() {
        options.push(2);
    }
    if options.is_empty() {
        return Statement::Assume {
            condition: "true".into(),
        };
    }

    let option = tc.draw(generators::sampled_from(options));
    match option {
        0 => {
            let var = Env::pick_from(tc, &int_vars);
            Statement::Assume {
                condition: format!("{} != {}::MAX", var.name, var.rust_type.render()),
            }
        }
        1 => {
            let var = Env::pick_from(tc, &collection_vars);
            Statement::Assume {
                condition: format!("{}.len() < 1000", var.name),
            }
        }
        2 => {
            let var = Env::pick_from(tc, &string_vars);
            Statement::Assume {
                condition: format!("{}.len() < 10000", var.name),
            }
        }
        _ => unreachable!(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gen_expr::{GenExpr, IntType};

    #[test]
    fn test_draw_statement_render() {
        let stmt = Statement::Draw {
            var_name: "v0".into(),
            type_annotation: RustType::I32,
            gen_expr: GenExpr::Integers {
                int_type: IntType::I32,
                min: None,
                max: None,
            },
        };
        assert_eq!(
            stmt.render(),
            "let v0: i32 = tc.draw(generators::integers::<i32>());"
        );
    }

    #[test]
    fn test_assert_statement_render() {
        let stmt = Statement::Assert {
            condition: "v0 > 0".into(),
        };
        assert_eq!(stmt.render(), "assert!(v0 > 0);");
    }

    #[test]
    fn test_assert_eq_statement_render() {
        let stmt = Statement::AssertEq {
            left: "v0.wrapping_add(v1)".into(),
            right: "v1.wrapping_add(v0)".into(),
        };
        assert_eq!(
            stmt.render(),
            "assert_eq!(v0.wrapping_add(v1), v1.wrapping_add(v0));"
        );
    }

    #[test]
    fn test_assume_statement_render() {
        let stmt = Statement::Assume {
            condition: "v0 != i32::MAX".into(),
        };
        assert_eq!(stmt.render(), "tc.assume(v0 != i32::MAX);");
    }

    #[test]
    fn test_note_statement_render() {
        let stmt = Statement::Note {
            message: "\"v0 = {:?}\", v0".into(),
        };
        assert_eq!(stmt.render(), "tc.note(&format!(\"v0 = {:?}\", v0));");
    }

    #[test]
    fn test_dependent_draw_render() {
        let stmt = Statement::DependentDraw {
            var_name: "v1".into(),
            type_annotation: RustType::I32,
            gen_source:
                "generators::integers::<i32>().min_value(v0).max_value(v0.saturating_add(10_i32))"
                    .into(),
        };
        let rendered = stmt.render();
        assert!(rendered.starts_with("let v1: i32 = tc.draw("));
        assert!(rendered.contains("min_value(v0)"));
    }

    #[test]
    fn test_if_block_render() {
        let stmt = Statement::IfBlock {
            condition: "v0".into(),
            then_body: vec![Statement::Assert {
                condition: "true".into(),
            }],
            else_body: None,
        };
        let rendered = stmt.render();
        assert!(rendered.starts_with("if v0 {"));
        assert!(rendered.contains("assert!(true);"));
        assert!(rendered.ends_with("}"));
        assert!(!rendered.contains("else"));
    }

    #[test]
    fn test_if_block_with_else_render() {
        let stmt = Statement::IfBlock {
            condition: "v0 > 0".into(),
            then_body: vec![Statement::Assert {
                condition: "true".into(),
            }],
            else_body: Some(vec![Statement::Assert {
                condition: "false".into(),
            }]),
        };
        let rendered = stmt.render();
        assert!(rendered.contains("if v0 > 0 {"));
        assert!(rendered.contains("} else {"));
        assert!(rendered.contains("assert!(true);"));
        assert!(rendered.contains("assert!(false);"));
    }

    #[test]
    fn test_env_fresh_var() {
        let mut env = Env::new();
        assert_eq!(env.fresh_var(), "v0");
        assert_eq!(env.fresh_var(), "v1");
        assert_eq!(env.fresh_var(), "v2");
    }

    #[test]
    fn test_env_add_and_query_vars() {
        let mut env = Env::new();
        env.add_var("v0".into(), RustType::I32);
        env.add_var("v1".into(), RustType::Bool);
        env.add_var("v2".into(), RustType::String);
        env.add_var("v3".into(), RustType::Vec(Box::new(RustType::I32)));

        assert_eq!(env.integer_vars().len(), 1);
        assert_eq!(env.integer_vars()[0].name, "v0");

        assert_eq!(env.bool_vars().len(), 1);
        assert_eq!(env.bool_vars()[0].name, "v1");

        assert_eq!(env.string_vars().len(), 1);
        assert_eq!(env.string_vars()[0].name, "v2");

        assert_eq!(env.collection_vars().len(), 1);
        assert_eq!(env.collection_vars()[0].name, "v3");
    }

    #[test]
    fn test_env_save_restore() {
        let mut env = Env::new();
        env.add_var("v0".into(), RustType::I32);
        env.add_var("v1".into(), RustType::Bool);

        let save = env.save();
        assert_eq!(save, 2);

        env.add_var("v2".into(), RustType::String);
        assert_eq!(env.vars.len(), 3);

        env.restore(save);
        assert_eq!(env.vars.len(), 2);
        assert_eq!(env.vars[0].name, "v0");
        assert_eq!(env.vars[1].name, "v1");
    }

    #[test]
    fn test_env_vars_of_type() {
        let mut env = Env::new();
        env.add_var("a".into(), RustType::I32);
        env.add_var("b".into(), RustType::I64);
        env.add_var("c".into(), RustType::F64);
        env.add_var("d".into(), RustType::U8);

        let ints = env.vars_of_type(|t| t.is_integer());
        assert_eq!(ints.len(), 3);

        let floats = env.vars_of_type(|t| t.is_float());
        assert_eq!(floats.len(), 1);
        assert_eq!(floats[0].name, "c");
    }
}
