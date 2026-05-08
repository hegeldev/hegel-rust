use crate::types::{self, RustType};
use hegel::TestCase;
use hegel::generators;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntType {
    I8,
    I16,
    I32,
    I64,
    I128,
    U8,
    U16,
    U32,
    U64,
    U128,
    Isize,
    Usize,
}

impl IntType {
    pub fn rust_type(self) -> RustType {
        match self {
            IntType::I8 => RustType::I8,
            IntType::I16 => RustType::I16,
            IntType::I32 => RustType::I32,
            IntType::I64 => RustType::I64,
            IntType::I128 => RustType::I128,
            IntType::U8 => RustType::U8,
            IntType::U16 => RustType::U16,
            IntType::U32 => RustType::U32,
            IntType::U64 => RustType::U64,
            IntType::U128 => RustType::U128,
            IntType::Isize => RustType::Isize,
            IntType::Usize => RustType::Usize,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            IntType::I8 => "i8",
            IntType::I16 => "i16",
            IntType::I32 => "i32",
            IntType::I64 => "i64",
            IntType::I128 => "i128",
            IntType::U8 => "u8",
            IntType::U16 => "u16",
            IntType::U32 => "u32",
            IntType::U64 => "u64",
            IntType::U128 => "u128",
            IntType::Isize => "isize",
            IntType::Usize => "usize",
        }
    }

    pub fn is_signed(self) -> bool {
        matches!(
            self,
            IntType::I8
                | IntType::I16
                | IntType::I32
                | IntType::I64
                | IntType::I128
                | IntType::Isize
        )
    }

    /// Return safe small bounds for this integer type.
    /// Returns (low_min, low_max, delta_max) such that
    /// we draw low in [low_min, low_max] and delta in [0, delta_max].
    pub fn safe_bounds(self) -> (i128, i128, i128) {
        match self {
            IntType::I8 => (-50, 50, 100),
            IntType::I16 => (-500, 500, 1000),
            IntType::I32 => (-10000, 10000, 20000),
            IntType::I64 => (-100000, 100000, 200000),
            IntType::I128 => (-100000, 100000, 200000),
            IntType::Isize => (-10000, 10000, 20000),
            IntType::U8 => (0, 100, 155),
            IntType::U16 => (0, 1000, 64535),
            IntType::U32 => (0, 10000, 50000),
            IntType::U64 => (0, 100000, 200000),
            IntType::U128 => (0, 100000, 200000),
            IntType::Usize => (0, 10000, 50000),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FloatType {
    F32,
    F64,
}

impl FloatType {
    pub fn rust_type(self) -> RustType {
        match self {
            FloatType::F32 => RustType::F32,
            FloatType::F64 => RustType::F64,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            FloatType::F32 => "f32",
            FloatType::F64 => "f64",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IpVersion {
    V4,
    V6,
}

/// A generator expression that renders to Rust source code.
#[derive(Debug, Clone)]
pub enum GenExpr {
    // Primitives
    Booleans,
    Integers {
        int_type: IntType,
        min: Option<String>,
        max: Option<String>,
    },
    Floats {
        float_type: FloatType,
        min: Option<String>,
        max: Option<String>,
        allow_nan: bool,
        allow_infinity: bool,
    },
    Text {
        min_size: Option<usize>,
        max_size: Option<usize>,
    },
    Binary {
        min_size: Option<usize>,
        max_size: Option<usize>,
    },
    Just {
        value: String,
        rust_type: RustType,
    },

    // String-like
    Emails,
    Urls,
    Domains {
        max_length: Option<usize>,
    },
    IpAddresses {
        version: Option<IpVersion>,
    },
    Dates,
    Times,
    DateTimes,
    FromRegex {
        pattern: String,
        fullmatch: bool,
    },

    // Collections
    Vecs {
        element: Box<GenExpr>,
        min_size: Option<usize>,
        max_size: Option<usize>,
        unique: bool,
    },
    HashSets {
        element: Box<GenExpr>,
        min_size: Option<usize>,
        max_size: Option<usize>,
    },
    HashMaps {
        key: Box<GenExpr>,
        value: Box<GenExpr>,
        min_size: Option<usize>,
        max_size: Option<usize>,
    },

    // Combinators
    Optional {
        inner: Box<GenExpr>,
    },
    SampledFrom {
        values: Vec<String>,
        rust_type: RustType,
    },
    OneOf {
        variants: Vec<GenExpr>,
    },
    Mapped {
        source: Box<GenExpr>,
        closure: String,
        result_type: RustType,
    },
    Filtered {
        source: Box<GenExpr>,
        predicate: String,
    },
    FlatMapped {
        source: Box<GenExpr>,
        closure: String,
        result_type: RustType,
    },

    // Composition
    Compose {
        body: String,
        result_type: RustType,
    },
}

impl GenExpr {
    pub fn output_type(&self) -> RustType {
        match self {
            GenExpr::Booleans => RustType::Bool,
            GenExpr::Integers { int_type, .. } => int_type.rust_type(),
            GenExpr::Floats { float_type, .. } => float_type.rust_type(),
            GenExpr::Text { .. } => RustType::String,
            GenExpr::Binary { .. } => RustType::VecU8,
            GenExpr::Just { rust_type, .. } => rust_type.clone(),
            GenExpr::Emails => RustType::String,
            GenExpr::Urls => RustType::String,
            GenExpr::Domains { .. } => RustType::String,
            GenExpr::IpAddresses { .. } => RustType::String,
            GenExpr::Dates => RustType::String,
            GenExpr::Times => RustType::String,
            GenExpr::DateTimes => RustType::String,
            GenExpr::FromRegex { .. } => RustType::String,
            GenExpr::Vecs { element, .. } => RustType::Vec(Box::new(element.output_type())),
            GenExpr::HashSets { element, .. } => RustType::HashSet(Box::new(element.output_type())),
            GenExpr::HashMaps { key, value, .. } => {
                RustType::HashMap(Box::new(key.output_type()), Box::new(value.output_type()))
            }
            GenExpr::Optional { inner } => RustType::Option(Box::new(inner.output_type())),
            GenExpr::SampledFrom { rust_type, .. } => rust_type.clone(),
            GenExpr::OneOf { variants } => variants[0].output_type(),
            GenExpr::Mapped { result_type, .. } => result_type.clone(),
            GenExpr::Filtered { source, .. } => source.output_type(),
            GenExpr::FlatMapped { result_type, .. } => result_type.clone(),
            GenExpr::Compose { result_type, .. } => result_type.clone(),
        }
    }

    pub fn render(&self) -> String {
        match self {
            GenExpr::Booleans => "generators::booleans()".into(),

            GenExpr::Integers { int_type, min, max } => {
                let mut s = format!("generators::integers::<{}>()", int_type.name());
                if let Some(min) = min {
                    s.push_str(&format!(".min_value({min})"));
                }
                if let Some(max) = max {
                    s.push_str(&format!(".max_value({max})"));
                }
                s
            }

            GenExpr::Floats {
                float_type,
                min,
                max,
                allow_nan,
                allow_infinity,
            } => {
                let mut s = format!("generators::floats::<{}>()", float_type.name());
                if let Some(min) = min {
                    s.push_str(&format!(".min_value({min})"));
                }
                if let Some(max) = max {
                    s.push_str(&format!(".max_value({max})"));
                }
                // Only emit allow_nan/allow_infinity when they differ from defaults
                let has_min = min.is_some();
                let has_max = max.is_some();
                let default_nan = !has_min && !has_max;
                let default_inf = !has_min || !has_max;
                if *allow_nan != default_nan {
                    s.push_str(&format!(".allow_nan({allow_nan})"));
                }
                if *allow_infinity != default_inf {
                    s.push_str(&format!(".allow_infinity({allow_infinity})"));
                }
                s
            }

            GenExpr::Text { min_size, max_size } => {
                let mut s = "generators::text()".to_string();
                if let Some(min) = min_size {
                    s.push_str(&format!(".min_size({min})"));
                }
                if let Some(max) = max_size {
                    s.push_str(&format!(".max_size({max})"));
                }
                s
            }

            GenExpr::Binary { min_size, max_size } => {
                let mut s = "generators::binary()".to_string();
                if let Some(min) = min_size {
                    s.push_str(&format!(".min_size({min})"));
                }
                if let Some(max) = max_size {
                    s.push_str(&format!(".max_size({max})"));
                }
                s
            }

            GenExpr::Just { value, .. } => format!("generators::just({value})"),

            GenExpr::Emails => "generators::emails()".into(),
            GenExpr::Urls => "generators::urls()".into(),

            GenExpr::Domains { max_length } => {
                let mut s = "generators::domains()".to_string();
                if let Some(len) = max_length {
                    s.push_str(&format!(".max_length({len})"));
                }
                s
            }

            GenExpr::IpAddresses { version } => {
                let mut s = "generators::ip_addresses()".to_string();
                match version {
                    Some(IpVersion::V4) => s.push_str(".v4()"),
                    Some(IpVersion::V6) => s.push_str(".v6()"),
                    None => {}
                }
                s
            }

            GenExpr::Dates => "generators::dates()".into(),
            GenExpr::Times => "generators::times()".into(),
            GenExpr::DateTimes => "generators::datetimes()".into(),

            GenExpr::FromRegex { pattern, fullmatch } => {
                let mut s = format!("generators::from_regex({pattern:?})");
                if *fullmatch {
                    s.push_str(".fullmatch(true)");
                }
                s
            }

            GenExpr::Vecs {
                element,
                min_size,
                max_size,
                unique,
            } => {
                let mut s = format!("generators::vecs({})", element.render());
                if let Some(min) = min_size {
                    s.push_str(&format!(".min_size({min})"));
                }
                if let Some(max) = max_size {
                    s.push_str(&format!(".max_size({max})"));
                }
                if *unique {
                    s.push_str(".unique(true)");
                }
                s
            }

            GenExpr::HashSets {
                element,
                min_size,
                max_size,
            } => {
                let mut s = format!("generators::hashsets({})", element.render());
                if let Some(min) = min_size {
                    s.push_str(&format!(".min_size({min})"));
                }
                if let Some(max) = max_size {
                    s.push_str(&format!(".max_size({max})"));
                }
                s
            }

            GenExpr::HashMaps {
                key,
                value,
                min_size,
                max_size,
            } => {
                let mut s = format!("generators::hashmaps({}, {})", key.render(), value.render());
                if let Some(min) = min_size {
                    s.push_str(&format!(".min_size({min})"));
                }
                if let Some(max) = max_size {
                    s.push_str(&format!(".max_size({max})"));
                }
                s
            }

            GenExpr::Optional { inner } => {
                format!("generators::optional({})", inner.render())
            }

            GenExpr::SampledFrom { values, .. } => {
                let vals = values.join(", ");
                format!("generators::sampled_from(vec![{vals}])")
            }

            GenExpr::OneOf { variants } => {
                let parts: Vec<_> = variants
                    .iter()
                    .map(|v| format!("{}.boxed()", v.render()))
                    .collect();
                format!("generators::one_of(vec![{}])", parts.join(", "))
            }

            GenExpr::Mapped {
                source, closure, ..
            } => {
                format!("{}.map({closure})", source.render())
            }

            GenExpr::Filtered {
                source, predicate, ..
            } => {
                format!("{}.filter({predicate})", source.render())
            }

            GenExpr::FlatMapped {
                source, closure, ..
            } => {
                format!("{}.flat_map({closure})", source.render())
            }

            GenExpr::Compose { body, .. } => {
                format!("hegel::compose!(|tc| {{\n{body}\n    }})")
            }
        }
    }
}

// ---- Generation of GenExpr using hegel ----

fn int_type_from_rust_type(rt: &RustType) -> IntType {
    match rt {
        RustType::I8 => IntType::I8,
        RustType::I16 => IntType::I16,
        RustType::I32 => IntType::I32,
        RustType::I64 => IntType::I64,
        RustType::I128 => IntType::I128,
        RustType::U8 => IntType::U8,
        RustType::U16 => IntType::U16,
        RustType::U32 => IntType::U32,
        RustType::U64 => IntType::U64,
        RustType::U128 => IntType::U128,
        RustType::Isize => IntType::Isize,
        RustType::Usize => IntType::Usize,
        _ => panic!("not an integer type: {rt:?}"),
    }
}

/// Generate a GenExpr for a given RustType, with bounded nesting.
pub fn gen_expr_for_type(tc: &TestCase, rt: &RustType, depth: usize) -> GenExpr {
    match rt {
        RustType::Bool => gen_bool_expr(tc, depth),
        t if t.is_integer() => gen_integer_expr(tc, int_type_from_rust_type(t), depth),
        RustType::F32 => gen_float_expr(tc, FloatType::F32, depth),
        RustType::F64 => gen_float_expr(tc, FloatType::F64, depth),
        RustType::String => gen_string_expr(tc, depth),
        RustType::VecU8 => gen_binary_expr(tc),
        RustType::Vec(inner) => gen_vec_expr(tc, inner, depth),
        RustType::HashSet(inner) => gen_hashset_expr(tc, inner, depth),
        RustType::HashMap(k, v) => gen_hashmap_expr(tc, k, v, depth),
        RustType::Option(inner) => gen_optional_expr(tc, inner, depth),
        RustType::Tuple(elems) => gen_tuple_expr(tc, elems, depth),
        _ => unreachable!("unsupported type: {rt:?}"),
    }
}

fn gen_bool_expr(tc: &TestCase, depth: usize) -> GenExpr {
    if depth > 0 {
        let choice: u8 = tc.draw(generators::integers::<u8>().min_value(0).max_value(3));
        match choice {
            0 | 1 => GenExpr::Booleans,
            2 => GenExpr::SampledFrom {
                values: vec!["true".into(), "false".into()],
                rust_type: RustType::Bool,
            },
            3 => GenExpr::Just {
                value: if tc.draw(generators::booleans()) {
                    "true".into()
                } else {
                    "false".into()
                },
                rust_type: RustType::Bool,
            },
            _ => unreachable!(),
        }
    } else {
        GenExpr::Booleans
    }
}

fn gen_integer_expr(tc: &TestCase, int_type: IntType, depth: usize) -> GenExpr {
    let choice: u8 = if depth > 0 {
        tc.draw(generators::integers::<u8>().min_value(0).max_value(7))
    } else {
        tc.draw(generators::integers::<u8>().min_value(0).max_value(2))
    };

    match choice {
        // Bare integers
        0 => GenExpr::Integers {
            int_type,
            min: None,
            max: None,
        },
        // Bounded integers
        1 | 2 => {
            let (low_min, low_max, delta_max) = int_type.safe_bounds();
            let low: i128 = tc.draw(
                generators::integers::<i128>()
                    .min_value(low_min)
                    .max_value(low_max),
            );
            let delta: i128 = tc.draw(
                generators::integers::<i128>()
                    .min_value(0)
                    .max_value(delta_max),
            );
            let high = low + delta;
            GenExpr::Integers {
                int_type,
                min: Some(format!("{low}_{}", int_type.name())),
                max: Some(format!("{high}_{}", int_type.name())),
            }
        }
        // sampled_from with small literal set
        3 => {
            let (low_min, low_max, _) = int_type.safe_bounds();
            let n: usize = tc.draw(generators::integers::<usize>().min_value(2).max_value(5));
            let values: Vec<String> = (0..n)
                .map(|_| {
                    let v: i128 = tc.draw(
                        generators::integers::<i128>()
                            .min_value(low_min)
                            .max_value(low_max),
                    );
                    format!("{v}_{}", int_type.name())
                })
                .collect();
            GenExpr::SampledFrom {
                values,
                rust_type: int_type.rust_type(),
            }
        }
        // one_of with two bounded integer ranges
        4 => {
            let g1 = gen_integer_expr(tc, int_type, 0);
            let g2 = gen_integer_expr(tc, int_type, 0);
            GenExpr::OneOf {
                variants: vec![g1, g2],
            }
        }
        // mapped integer (wrapping_add)
        5 => {
            let source = gen_integer_expr(tc, int_type, 0);
            GenExpr::Mapped {
                source: Box::new(source),
                closure: "|n| n.wrapping_add(1)".into(),
                result_type: int_type.rust_type(),
            }
        }
        // filtered integer
        6 => {
            let source = gen_integer_expr(tc, int_type, 0);
            let predicate = if int_type.is_signed() {
                "|n| *n >= 0"
            } else {
                "|_| true"
            };
            GenExpr::Filtered {
                source: Box::new(source),
                predicate: predicate.into(),
            }
        }
        // just a constant
        7 => {
            let (low_min, low_max, _) = int_type.safe_bounds();
            let v: i128 = tc.draw(
                generators::integers::<i128>()
                    .min_value(low_min)
                    .max_value(low_max),
            );
            GenExpr::Just {
                value: format!("{v}_{}", int_type.name()),
                rust_type: int_type.rust_type(),
            }
        }
        _ => unreachable!(),
    }
}

fn gen_float_expr(tc: &TestCase, float_type: FloatType, depth: usize) -> GenExpr {
    let choice: u8 = if depth > 0 {
        tc.draw(generators::integers::<u8>().min_value(0).max_value(4))
    } else {
        tc.draw(generators::integers::<u8>().min_value(0).max_value(2))
    };

    match choice {
        // Bare floats (allows NaN and infinity by default)
        0 => GenExpr::Floats {
            float_type,
            min: None,
            max: None,
            allow_nan: true,
            allow_infinity: true,
        },
        // Bounded floats (no NaN, no infinity)
        1 => {
            let low: f64 = tc.draw(
                generators::floats::<f64>()
                    .min_value(-1000.0)
                    .max_value(1000.0),
            );
            let delta: f64 = tc.draw(generators::floats::<f64>().min_value(0.0).max_value(1000.0));
            let high = low + delta;
            GenExpr::Floats {
                float_type,
                min: Some(format!("{low}_{}", float_type.name())),
                max: Some(format!("{high}_{}", float_type.name())),
                allow_nan: false,
                allow_infinity: false,
            }
        }
        // Finite floats (no NaN, no infinity, no explicit bounds)
        2 => GenExpr::Floats {
            float_type,
            min: None,
            max: None,
            allow_nan: false,
            allow_infinity: false,
        },
        // One-sided bound (no NaN, infinity allowed)
        3 => {
            if tc.draw(generators::booleans()) {
                let low: f64 = tc.draw(
                    generators::floats::<f64>()
                        .min_value(-1000.0)
                        .max_value(1000.0),
                );
                GenExpr::Floats {
                    float_type,
                    min: Some(format!("{low}_{}", float_type.name())),
                    max: None,
                    allow_nan: false,
                    allow_infinity: true,
                }
            } else {
                let high: f64 = tc.draw(
                    generators::floats::<f64>()
                        .min_value(-1000.0)
                        .max_value(1000.0),
                );
                GenExpr::Floats {
                    float_type,
                    min: None,
                    max: Some(format!("{high}_{}", float_type.name())),
                    allow_nan: false,
                    allow_infinity: true,
                }
            }
        }
        // mapped float
        4 => {
            let source = gen_float_expr(tc, float_type, 0);
            GenExpr::Mapped {
                source: Box::new(source),
                closure: "|n| n * 2.0".into(),
                result_type: float_type.rust_type(),
            }
        }
        _ => unreachable!(),
    }
}

fn gen_string_expr(tc: &TestCase, depth: usize) -> GenExpr {
    let choice: u8 = if depth > 0 {
        tc.draw(generators::integers::<u8>().min_value(0).max_value(14))
    } else {
        tc.draw(generators::integers::<u8>().min_value(0).max_value(4))
    };

    match choice {
        // text()
        0 => GenExpr::Text {
            min_size: None,
            max_size: None,
        },
        // bounded text
        1 | 2 => {
            let min: usize = tc.draw(generators::integers::<usize>().min_value(0).max_value(5));
            let delta: usize = tc.draw(generators::integers::<usize>().min_value(0).max_value(50));
            GenExpr::Text {
                min_size: Some(min),
                max_size: Some(min + delta),
            }
        }
        // text with only max_size
        3 => {
            let max: usize = tc.draw(generators::integers::<usize>().min_value(0).max_value(100));
            GenExpr::Text {
                min_size: None,
                max_size: Some(max),
            }
        }
        // text with only min_size
        4 => {
            let min: usize = tc.draw(generators::integers::<usize>().min_value(0).max_value(5));
            GenExpr::Text {
                min_size: Some(min),
                max_size: None,
            }
        }
        // emails
        5 => GenExpr::Emails,
        // urls
        6 => GenExpr::Urls,
        // domains
        7 => {
            let use_max: bool = tc.draw(generators::booleans());
            GenExpr::Domains {
                max_length: if use_max {
                    Some(tc.draw(generators::integers::<usize>().min_value(4).max_value(255)))
                } else {
                    None
                },
            }
        }
        // ip_addresses
        8 => {
            let choice: u8 = tc.draw(generators::integers::<u8>().min_value(0).max_value(2));
            GenExpr::IpAddresses {
                version: match choice {
                    0 => None,
                    1 => Some(IpVersion::V4),
                    2 => Some(IpVersion::V6),
                    _ => unreachable!(),
                },
            }
        }
        // dates
        9 => GenExpr::Dates,
        // times
        10 => GenExpr::Times,
        // datetimes
        11 => GenExpr::DateTimes,
        // from_regex with safe pattern
        12 => {
            let pattern: String = tc.draw(generators::sampled_from(vec![
                "[a-z]+".to_string(),
                "[0-9]{1,5}".to_string(),
                "[A-Za-z0-9_]+".to_string(),
                "(foo|bar|baz)".to_string(),
                "[a-z]{2,4}@[a-z]{2,4}".to_string(),
                "\\d{3}-\\d{4}".to_string(),
            ]));
            let fullmatch: bool = tc.draw(generators::booleans());
            GenExpr::FromRegex { pattern, fullmatch }
        }
        // one_of string generators
        13 => {
            let g1 = gen_string_expr(tc, 0);
            let g2 = gen_string_expr(tc, 0);
            GenExpr::OneOf {
                variants: vec![g1, g2],
            }
        }
        // mapped string
        14 => {
            let source = gen_string_expr(tc, 0);
            let closure: String = tc.draw(generators::sampled_from(vec![
                "|s| s.to_uppercase()".to_string(),
                "|s| s.to_lowercase()".to_string(),
                "|s| format!(\"prefix_{}\", s)".to_string(),
            ]));
            GenExpr::Mapped {
                source: Box::new(source),
                closure,
                result_type: RustType::String,
            }
        }
        _ => unreachable!(),
    }
}

fn gen_binary_expr(tc: &TestCase) -> GenExpr {
    let use_bounds: bool = tc.draw(generators::booleans());
    if use_bounds {
        let min: usize = tc.draw(generators::integers::<usize>().min_value(0).max_value(5));
        let delta: usize = tc.draw(generators::integers::<usize>().min_value(0).max_value(100));
        GenExpr::Binary {
            min_size: Some(min),
            max_size: Some(min + delta),
        }
    } else {
        GenExpr::Binary {
            min_size: None,
            max_size: None,
        }
    }
}

fn gen_vec_expr(tc: &TestCase, inner: &RustType, depth: usize) -> GenExpr {
    let elem = gen_expr_for_type(tc, inner, depth.saturating_sub(1));
    let use_bounds: bool = tc.draw(generators::booleans());
    let (min_size, max_size) = if use_bounds {
        let min: usize = tc.draw(generators::integers::<usize>().min_value(0).max_value(5));
        let delta: usize = tc.draw(generators::integers::<usize>().min_value(0).max_value(20));
        (Some(min), Some(min + delta))
    } else {
        (None, None)
    };
    let unique = if inner.is_hashable() && inner.is_eq() {
        tc.draw(generators::booleans())
    } else {
        false
    };
    GenExpr::Vecs {
        element: Box::new(elem),
        min_size,
        max_size,
        unique,
    }
}

fn gen_hashset_expr(tc: &TestCase, inner: &RustType, depth: usize) -> GenExpr {
    let elem = gen_expr_for_type(tc, inner, depth.saturating_sub(1));
    let use_bounds: bool = tc.draw(generators::booleans());
    let (min_size, max_size) = if use_bounds {
        let min: usize = tc.draw(generators::integers::<usize>().min_value(0).max_value(3));
        let delta: usize = tc.draw(generators::integers::<usize>().min_value(0).max_value(10));
        (Some(min), Some(min + delta))
    } else {
        (None, None)
    };
    GenExpr::HashSets {
        element: Box::new(elem),
        min_size,
        max_size,
    }
}

fn gen_hashmap_expr(
    tc: &TestCase,
    key_type: &RustType,
    val_type: &RustType,
    depth: usize,
) -> GenExpr {
    let key = gen_expr_for_type(tc, key_type, depth.saturating_sub(1));
    let value = gen_expr_for_type(tc, val_type, depth.saturating_sub(1));
    let use_bounds: bool = tc.draw(generators::booleans());
    let (min_size, max_size) = if use_bounds {
        let min: usize = tc.draw(generators::integers::<usize>().min_value(0).max_value(3));
        let delta: usize = tc.draw(generators::integers::<usize>().min_value(0).max_value(10));
        (Some(min), Some(min + delta))
    } else {
        (None, None)
    };
    GenExpr::HashMaps {
        key: Box::new(key),
        value: Box::new(value),
        min_size,
        max_size,
    }
}

fn gen_optional_expr(tc: &TestCase, inner: &RustType, depth: usize) -> GenExpr {
    let inner_gen = gen_expr_for_type(tc, inner, depth.saturating_sub(1));
    GenExpr::Optional {
        inner: Box::new(inner_gen),
    }
}

fn gen_tuple_expr(tc: &TestCase, elems: &[RustType], depth: usize) -> GenExpr {
    // For tuples, we generate a compose! block that draws each element
    let mut lines = Vec::new();
    let mut vars = Vec::new();
    for (i, elem) in elems.iter().enumerate() {
        let var = format!("t{i}");
        let ge = gen_expr_for_type(tc, elem, depth.saturating_sub(1));
        lines.push(format!(
            "        let {var}: {} = tc.draw({});",
            elem.render(),
            ge.render()
        ));
        vars.push(var);
    }
    let result = format!("        ({})", vars.join(", "));
    lines.push(result);
    GenExpr::Compose {
        body: lines.join("\n"),
        result_type: RustType::Tuple(elems.to_vec()),
    }
}

/// Generate a compose! block with dependent draws between variables.
pub fn gen_compose_expr(tc: &TestCase, depth: usize) -> GenExpr {
    let choice: u8 = tc.draw(generators::integers::<u8>().min_value(0).max_value(3));
    match choice {
        // Draw a bound, then draw a value within that bound
        0 => {
            let int_type = tc.draw(generators::sampled_from(vec![
                IntType::I32,
                IntType::U32,
                IntType::Usize,
            ]));
            let type_name = int_type.name();
            let (_, _, delta_max) = int_type.safe_bounds();
            let delta_max = delta_max.min(100);
            let body = format!(
                "        let lo: {type_name} = tc.draw(generators::integers::<{type_name}>().min_value(0_{type_name}).max_value({delta_max}_{type_name}));\n        \
                 let hi: {type_name} = tc.draw(generators::integers::<{type_name}>().min_value(lo).max_value(lo.saturating_add({delta_max}_{type_name})));\n        \
                 let val: {type_name} = tc.draw(generators::integers::<{type_name}>().min_value(lo).max_value(hi));\n        \
                 val"
            );
            GenExpr::Compose {
                body,
                result_type: int_type.rust_type(),
            }
        }
        // Draw a size, then draw a vec of that size
        1 => {
            let elem_type = types::gen_leaf_type(tc);
            let elem_gen = gen_expr_for_type(tc, &elem_type, depth.saturating_sub(1));
            let body = format!(
                "        let size: usize = tc.draw(generators::integers::<usize>().min_value(1).max_value(5));\n        \
                 let items: Vec<{}> = tc.draw(generators::vecs({}).min_size(size).max_size(size));\n        \
                 items",
                elem_type.render(),
                elem_gen.render()
            );
            GenExpr::Compose {
                body,
                result_type: RustType::Vec(Box::new(elem_type)),
            }
        }
        // Draw two values and combine them
        2 => {
            let body =
                "        let a: i32 = tc.draw(generators::integers::<i32>().min_value(-100).max_value(100));\n        \
                 let b: i32 = tc.draw(generators::integers::<i32>().min_value(-100).max_value(100));\n        \
                 a.wrapping_add(b)".to_string();
            GenExpr::Compose {
                body,
                result_type: RustType::I32,
            }
        }
        // Draw a bool and conditionally generate different values
        3 => {
            let body = "        let flag: bool = tc.draw(generators::booleans());\n        \
                 if flag {\n            \
                     tc.draw(generators::text().min_size(1).max_size(5))\n        \
                 } else {\n            \
                     tc.draw(generators::text().min_size(10).max_size(20))\n        \
                 }"
            .to_string();
            GenExpr::Compose {
                body,
                result_type: RustType::String,
            }
        }
        _ => unreachable!(),
    }
}

/// Generate a flat_map expression (dependent generation).
pub fn gen_flat_map_expr(tc: &TestCase) -> GenExpr {
    let choice: u8 = tc.draw(generators::integers::<u8>().min_value(0).max_value(4));
    match choice {
        // usize -> text with bounded size
        0 => {
            let source = GenExpr::Integers {
                int_type: IntType::Usize,
                min: Some("1_usize".into()),
                max: Some("5_usize".into()),
            };
            GenExpr::FlatMapped {
                source: Box::new(source),
                closure: "|n| generators::text().min_size(0).max_size(n)".into(),
                result_type: RustType::String,
            }
        }
        // bool -> integer with conditional bounds
        1 => {
            let source = GenExpr::Booleans;
            GenExpr::FlatMapped {
                source: Box::new(source),
                closure: "|b| if b { generators::integers::<i32>().min_value(0).max_value(100) } else { generators::integers::<i32>().min_value(-100).max_value(0) }".into(),
                result_type: RustType::I32,
            }
        }
        // usize -> vec with bounded size
        2 => {
            let source = GenExpr::Integers {
                int_type: IntType::Usize,
                min: Some("1_usize".into()),
                max: Some("10_usize".into()),
            };
            GenExpr::FlatMapped {
                source: Box::new(source),
                closure: "|n| generators::vecs(generators::booleans()).min_size(n).max_size(n)"
                    .into(),
                result_type: RustType::Vec(Box::new(RustType::Bool)),
            }
        }
        // i32 -> bounded i32 (dependent range)
        3 => {
            let source = GenExpr::Integers {
                int_type: IntType::I32,
                min: Some("0_i32".into()),
                max: Some("100_i32".into()),
            };
            GenExpr::FlatMapped {
                source: Box::new(source),
                closure: "|lo| generators::integers::<i32>().min_value(lo).max_value(lo.saturating_add(50))".into(),
                result_type: RustType::I32,
            }
        }
        // usize -> binary with bounded size
        4 => {
            let source = GenExpr::Integers {
                int_type: IntType::Usize,
                min: Some("0_usize".into()),
                max: Some("20_usize".into()),
            };
            GenExpr::FlatMapped {
                source: Box::new(source),
                closure: "|n| generators::binary().min_size(0).max_size(n)".into(),
                result_type: RustType::VecU8,
            }
        }
        _ => unreachable!(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_booleans_render() {
        assert_eq!(GenExpr::Booleans.render(), "generators::booleans()");
    }

    #[test]
    fn test_booleans_output_type() {
        assert_eq!(GenExpr::Booleans.output_type(), RustType::Bool);
    }

    #[test]
    fn test_integers_bare_render() {
        let expr = GenExpr::Integers {
            int_type: IntType::I32,
            min: None,
            max: None,
        };
        assert_eq!(expr.render(), "generators::integers::<i32>()");
    }

    #[test]
    fn test_integers_bounded_render() {
        let expr = GenExpr::Integers {
            int_type: IntType::U8,
            min: Some("0_u8".into()),
            max: Some("100_u8".into()),
        };
        assert_eq!(
            expr.render(),
            "generators::integers::<u8>().min_value(0_u8).max_value(100_u8)"
        );
    }

    #[test]
    fn test_integers_output_type() {
        let expr = GenExpr::Integers {
            int_type: IntType::I64,
            min: None,
            max: None,
        };
        assert_eq!(expr.output_type(), RustType::I64);
    }

    #[test]
    fn test_floats_bare_render() {
        let expr = GenExpr::Floats {
            float_type: FloatType::F64,
            min: None,
            max: None,
            allow_nan: true,
            allow_infinity: true,
        };
        assert_eq!(expr.render(), "generators::floats::<f64>()");
    }

    #[test]
    fn test_floats_bounded_render() {
        let expr = GenExpr::Floats {
            float_type: FloatType::F32,
            min: Some("-1.0_f32".into()),
            max: Some("1.0_f32".into()),
            allow_nan: false,
            allow_infinity: false,
        };
        assert_eq!(
            expr.render(),
            "generators::floats::<f32>().min_value(-1.0_f32).max_value(1.0_f32)"
        );
    }

    #[test]
    fn test_text_render() {
        assert_eq!(
            GenExpr::Text {
                min_size: None,
                max_size: None
            }
            .render(),
            "generators::text()"
        );
        assert_eq!(
            GenExpr::Text {
                min_size: Some(1),
                max_size: Some(10)
            }
            .render(),
            "generators::text().min_size(1).max_size(10)"
        );
    }

    #[test]
    fn test_binary_render() {
        assert_eq!(
            GenExpr::Binary {
                min_size: None,
                max_size: None
            }
            .render(),
            "generators::binary()"
        );
    }

    #[test]
    fn test_just_render() {
        let expr = GenExpr::Just {
            value: "42_i32".into(),
            rust_type: RustType::I32,
        };
        assert_eq!(expr.render(), "generators::just(42_i32)");
        assert_eq!(expr.output_type(), RustType::I32);
    }

    #[test]
    fn test_string_like_generators_render() {
        assert_eq!(GenExpr::Emails.render(), "generators::emails()");
        assert_eq!(GenExpr::Urls.render(), "generators::urls()");
        assert_eq!(GenExpr::Dates.render(), "generators::dates()");
        assert_eq!(GenExpr::Times.render(), "generators::times()");
        assert_eq!(GenExpr::DateTimes.render(), "generators::datetimes()");
    }

    #[test]
    fn test_domains_render() {
        assert_eq!(
            GenExpr::Domains { max_length: None }.render(),
            "generators::domains()"
        );
        assert_eq!(
            GenExpr::Domains {
                max_length: Some(63)
            }
            .render(),
            "generators::domains().max_length(63)"
        );
    }

    #[test]
    fn test_ip_addresses_render() {
        assert_eq!(
            GenExpr::IpAddresses { version: None }.render(),
            "generators::ip_addresses()"
        );
        assert_eq!(
            GenExpr::IpAddresses {
                version: Some(IpVersion::V4)
            }
            .render(),
            "generators::ip_addresses().v4()"
        );
        assert_eq!(
            GenExpr::IpAddresses {
                version: Some(IpVersion::V6)
            }
            .render(),
            "generators::ip_addresses().v6()"
        );
    }

    #[test]
    fn test_from_regex_render() {
        assert_eq!(
            GenExpr::FromRegex {
                pattern: "[a-z]+".into(),
                fullmatch: false
            }
            .render(),
            "generators::from_regex(\"[a-z]+\")"
        );
        assert_eq!(
            GenExpr::FromRegex {
                pattern: "[0-9]+".into(),
                fullmatch: true
            }
            .render(),
            "generators::from_regex(\"[0-9]+\").fullmatch(true)"
        );
    }

    #[test]
    fn test_vecs_render() {
        let expr = GenExpr::Vecs {
            element: Box::new(GenExpr::Booleans),
            min_size: Some(1),
            max_size: Some(5),
            unique: false,
        };
        assert_eq!(
            expr.render(),
            "generators::vecs(generators::booleans()).min_size(1).max_size(5)"
        );
        assert_eq!(expr.output_type(), RustType::Vec(Box::new(RustType::Bool)));
    }

    #[test]
    fn test_vecs_unique_render() {
        let expr = GenExpr::Vecs {
            element: Box::new(GenExpr::Integers {
                int_type: IntType::I32,
                min: None,
                max: None,
            }),
            min_size: None,
            max_size: None,
            unique: true,
        };
        assert_eq!(
            expr.render(),
            "generators::vecs(generators::integers::<i32>()).unique(true)"
        );
    }

    #[test]
    fn test_hashsets_render() {
        let expr = GenExpr::HashSets {
            element: Box::new(GenExpr::Integers {
                int_type: IntType::U32,
                min: None,
                max: None,
            }),
            min_size: None,
            max_size: Some(10),
        };
        assert_eq!(
            expr.render(),
            "generators::hashsets(generators::integers::<u32>()).max_size(10)"
        );
        assert_eq!(
            expr.output_type(),
            RustType::HashSet(Box::new(RustType::U32))
        );
    }

    #[test]
    fn test_hashmaps_render() {
        let expr = GenExpr::HashMaps {
            key: Box::new(GenExpr::Text {
                min_size: None,
                max_size: None,
            }),
            value: Box::new(GenExpr::Booleans),
            min_size: Some(0),
            max_size: Some(5),
        };
        assert_eq!(
            expr.render(),
            "generators::hashmaps(generators::text(), generators::booleans()).min_size(0).max_size(5)"
        );
        assert_eq!(
            expr.output_type(),
            RustType::HashMap(Box::new(RustType::String), Box::new(RustType::Bool))
        );
    }

    #[test]
    fn test_optional_render() {
        let expr = GenExpr::Optional {
            inner: Box::new(GenExpr::Booleans),
        };
        assert_eq!(
            expr.render(),
            "generators::optional(generators::booleans())"
        );
        assert_eq!(
            expr.output_type(),
            RustType::Option(Box::new(RustType::Bool))
        );
    }

    #[test]
    fn test_sampled_from_render() {
        let expr = GenExpr::SampledFrom {
            values: vec!["1_i32".into(), "2_i32".into(), "3_i32".into()],
            rust_type: RustType::I32,
        };
        assert_eq!(
            expr.render(),
            "generators::sampled_from(vec![1_i32, 2_i32, 3_i32])"
        );
    }

    #[test]
    fn test_one_of_render() {
        let expr = GenExpr::OneOf {
            variants: vec![
                GenExpr::Integers {
                    int_type: IntType::I32,
                    min: Some("0_i32".into()),
                    max: Some("10_i32".into()),
                },
                GenExpr::Integers {
                    int_type: IntType::I32,
                    min: Some("90_i32".into()),
                    max: Some("100_i32".into()),
                },
            ],
        };
        let rendered = expr.render();
        assert!(rendered.starts_with("generators::one_of(vec!["));
        assert!(rendered.contains(".boxed()"));
    }

    #[test]
    fn test_mapped_render() {
        let expr = GenExpr::Mapped {
            source: Box::new(GenExpr::Integers {
                int_type: IntType::I32,
                min: None,
                max: None,
            }),
            closure: "|n| n.wrapping_add(1)".into(),
            result_type: RustType::I32,
        };
        assert_eq!(
            expr.render(),
            "generators::integers::<i32>().map(|n| n.wrapping_add(1))"
        );
    }

    #[test]
    fn test_filtered_render() {
        let expr = GenExpr::Filtered {
            source: Box::new(GenExpr::Integers {
                int_type: IntType::I32,
                min: None,
                max: None,
            }),
            predicate: "|n| *n >= 0".into(),
        };
        assert_eq!(
            expr.render(),
            "generators::integers::<i32>().filter(|n| *n >= 0)"
        );
    }

    #[test]
    fn test_flat_mapped_render() {
        let expr = GenExpr::FlatMapped {
            source: Box::new(GenExpr::Integers {
                int_type: IntType::Usize,
                min: Some("1_usize".into()),
                max: Some("5_usize".into()),
            }),
            closure: "|n| generators::text().min_size(0).max_size(n)".into(),
            result_type: RustType::String,
        };
        let rendered = expr.render();
        assert!(rendered.contains(".flat_map("));
        assert_eq!(expr.output_type(), RustType::String);
    }

    #[test]
    fn test_compose_render() {
        let expr = GenExpr::Compose {
            body: "        let x = tc.draw(generators::booleans());\n        x".into(),
            result_type: RustType::Bool,
        };
        let rendered = expr.render();
        assert!(rendered.starts_with("hegel::compose!(|tc| {"));
        assert!(rendered.contains("let x = tc.draw(generators::booleans())"));
    }

    #[test]
    fn test_int_type_properties() {
        assert!(IntType::I32.is_signed());
        assert!(IntType::I8.is_signed());
        assert!(IntType::Isize.is_signed());
        assert!(!IntType::U32.is_signed());
        assert!(!IntType::Usize.is_signed());

        assert_eq!(IntType::I32.name(), "i32");
        assert_eq!(IntType::U64.name(), "u64");
        assert_eq!(IntType::Usize.name(), "usize");

        assert_eq!(IntType::I32.rust_type(), RustType::I32);
        assert_eq!(IntType::Usize.rust_type(), RustType::Usize);
    }

    #[test]
    fn test_float_type_properties() {
        assert_eq!(FloatType::F32.name(), "f32");
        assert_eq!(FloatType::F64.name(), "f64");
        assert_eq!(FloatType::F32.rust_type(), RustType::F32);
        assert_eq!(FloatType::F64.rust_type(), RustType::F64);
    }

    #[test]
    fn test_int_type_safe_bounds_unsigned_start_at_zero() {
        for int_type in [
            IntType::U8,
            IntType::U16,
            IntType::U32,
            IntType::U64,
            IntType::U128,
            IntType::Usize,
        ] {
            let (low_min, _, _) = int_type.safe_bounds();
            assert_eq!(low_min, 0, "{:?} should have low_min=0", int_type);
        }
    }

    #[test]
    fn test_int_type_safe_bounds_signed_go_negative() {
        for int_type in [
            IntType::I8,
            IntType::I16,
            IntType::I32,
            IntType::I64,
            IntType::I128,
            IntType::Isize,
        ] {
            let (low_min, _, _) = int_type.safe_bounds();
            assert!(low_min < 0, "{:?} should have negative low_min", int_type);
        }
    }

    #[test]
    fn test_string_like_output_types() {
        assert_eq!(GenExpr::Emails.output_type(), RustType::String);
        assert_eq!(GenExpr::Urls.output_type(), RustType::String);
        assert_eq!(
            GenExpr::Domains { max_length: None }.output_type(),
            RustType::String
        );
        assert_eq!(
            GenExpr::IpAddresses { version: None }.output_type(),
            RustType::String
        );
        assert_eq!(GenExpr::Dates.output_type(), RustType::String);
        assert_eq!(GenExpr::Times.output_type(), RustType::String);
        assert_eq!(GenExpr::DateTimes.output_type(), RustType::String);
        assert_eq!(
            GenExpr::FromRegex {
                pattern: ".*".into(),
                fullmatch: false
            }
            .output_type(),
            RustType::String
        );
    }
}
