use hegel::TestCase;
use hegel::generators;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RustType {
    Bool,
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
    F32,
    F64,
    String,
    VecU8,
    Vec(Box<RustType>),
    HashSet(Box<RustType>),
    HashMap(Box<RustType>, Box<RustType>),
    Option(Box<RustType>),
    Tuple(Vec<RustType>),
}

impl RustType {
    pub fn render(&self) -> std::string::String {
        match self {
            RustType::Bool => "bool".into(),
            RustType::I8 => "i8".into(),
            RustType::I16 => "i16".into(),
            RustType::I32 => "i32".into(),
            RustType::I64 => "i64".into(),
            RustType::I128 => "i128".into(),
            RustType::U8 => "u8".into(),
            RustType::U16 => "u16".into(),
            RustType::U32 => "u32".into(),
            RustType::U64 => "u64".into(),
            RustType::U128 => "u128".into(),
            RustType::Isize => "isize".into(),
            RustType::Usize => "usize".into(),
            RustType::F32 => "f32".into(),
            RustType::F64 => "f64".into(),
            RustType::String => "String".into(),
            RustType::VecU8 => "Vec<u8>".into(),
            RustType::Vec(inner) => format!("Vec<{}>", inner.render()),
            RustType::HashSet(inner) => format!("HashSet<{}>", inner.render()),
            RustType::HashMap(k, v) => format!("HashMap<{}, {}>", k.render(), v.render()),
            RustType::Option(inner) => format!("Option<{}>", inner.render()),
            RustType::Tuple(elems) => {
                let parts: Vec<_> = elems.iter().map(|e| e.render()).collect();
                format!("({})", parts.join(", "))
            }
        }
    }

    pub fn is_hashable(&self) -> bool {
        match self {
            RustType::Bool
            | RustType::I8
            | RustType::I16
            | RustType::I32
            | RustType::I64
            | RustType::I128
            | RustType::U8
            | RustType::U16
            | RustType::U32
            | RustType::U64
            | RustType::U128
            | RustType::Isize
            | RustType::Usize
            | RustType::String => true,
            RustType::VecU8 => true,
            RustType::Option(inner) => inner.is_hashable(),
            RustType::Tuple(elems) => elems.iter().all(|e| e.is_hashable()),
            RustType::F32 | RustType::F64 => false,
            RustType::Vec(_) | RustType::HashSet(_) | RustType::HashMap(_, _) => false,
        }
    }

    pub fn is_integer(&self) -> bool {
        matches!(
            self,
            RustType::I8
                | RustType::I16
                | RustType::I32
                | RustType::I64
                | RustType::I128
                | RustType::U8
                | RustType::U16
                | RustType::U32
                | RustType::U64
                | RustType::U128
                | RustType::Isize
                | RustType::Usize
        )
    }

    pub fn is_signed_int(&self) -> bool {
        matches!(
            self,
            RustType::I8
                | RustType::I16
                | RustType::I32
                | RustType::I64
                | RustType::I128
                | RustType::Isize
        )
    }

    #[allow(dead_code)]
    pub fn is_float(&self) -> bool {
        matches!(self, RustType::F32 | RustType::F64)
    }

    #[allow(dead_code)]
    pub fn is_ord(&self) -> bool {
        match self {
            RustType::F32 | RustType::F64 => false,
            RustType::Vec(inner) => inner.is_ord(),
            RustType::Option(inner) => inner.is_ord(),
            RustType::Tuple(elems) => elems.iter().all(|e| e.is_ord()),
            RustType::HashSet(_) | RustType::HashMap(_, _) => false,
            _ => true,
        }
    }

    pub fn is_eq(&self) -> bool {
        match self {
            RustType::F32 | RustType::F64 => false,
            RustType::Vec(inner) => inner.is_eq(),
            RustType::HashSet(inner) => inner.is_eq(),
            RustType::HashMap(k, v) => k.is_eq() && v.is_eq(),
            RustType::Option(inner) => inner.is_eq(),
            RustType::Tuple(elems) => elems.iter().all(|e| e.is_eq()),
            _ => true,
        }
    }

    pub fn is_collection(&self) -> bool {
        matches!(
            self,
            RustType::Vec(_) | RustType::VecU8 | RustType::HashSet(_) | RustType::HashMap(_, _)
        )
    }

    pub fn is_string_like(&self) -> bool {
        matches!(self, RustType::String)
    }
}

/// Generate a random leaf type (no nesting).
pub fn gen_leaf_type(tc: &TestCase) -> RustType {
    tc.draw(generators::sampled_from(vec![
        RustType::Bool,
        RustType::I8,
        RustType::I16,
        RustType::I32,
        RustType::I64,
        RustType::U8,
        RustType::U16,
        RustType::U32,
        RustType::U64,
        RustType::Usize,
        RustType::F32,
        RustType::F64,
        RustType::String,
    ]))
}

/// Generate a random hashable leaf type.
pub fn gen_hashable_leaf_type(tc: &TestCase) -> RustType {
    tc.draw(generators::sampled_from(vec![
        RustType::Bool,
        RustType::I8,
        RustType::I16,
        RustType::I32,
        RustType::I64,
        RustType::U8,
        RustType::U16,
        RustType::U32,
        RustType::U64,
        RustType::Usize,
        RustType::String,
    ]))
}

/// Generate a random type with bounded nesting depth.
pub fn gen_type(tc: &TestCase, depth: usize) -> RustType {
    if depth == 0 {
        return gen_leaf_type(tc);
    }

    let choice: u8 = tc.draw(generators::integers::<u8>().min_value(0).max_value(9));
    match choice {
        0..=5 => gen_leaf_type(tc),
        6 => {
            let inner = gen_type(tc, depth - 1);
            RustType::Vec(Box::new(inner))
        }
        7 => {
            let inner = gen_hashable_type(tc, depth - 1);
            RustType::HashSet(Box::new(inner))
        }
        8 => {
            let key = gen_hashable_type(tc, depth - 1);
            let value = gen_type(tc, depth - 1);
            RustType::HashMap(Box::new(key), Box::new(value))
        }
        9 => {
            let inner = gen_type(tc, depth - 1);
            RustType::Option(Box::new(inner))
        }
        _ => unreachable!(),
    }
}

/// Generate a random hashable type with bounded nesting depth.
pub fn gen_hashable_type(tc: &TestCase, depth: usize) -> RustType {
    if depth == 0 {
        return gen_hashable_leaf_type(tc);
    }

    let choice: u8 = tc.draw(generators::integers::<u8>().min_value(0).max_value(7));
    match choice {
        0..=5 => gen_hashable_leaf_type(tc),
        6 => {
            let inner = gen_hashable_type(tc, depth - 1);
            RustType::Option(Box::new(inner))
        }
        7 => {
            let n: usize = tc.draw(generators::integers::<usize>().min_value(2).max_value(4));
            let elems: Vec<RustType> = (0..n).map(|_| gen_hashable_type(tc, depth - 1)).collect();
            RustType::Tuple(elems)
        }
        _ => unreachable!(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_render_primitive_types() {
        assert_eq!(RustType::Bool.render(), "bool");
        assert_eq!(RustType::I8.render(), "i8");
        assert_eq!(RustType::I16.render(), "i16");
        assert_eq!(RustType::I32.render(), "i32");
        assert_eq!(RustType::I64.render(), "i64");
        assert_eq!(RustType::I128.render(), "i128");
        assert_eq!(RustType::U8.render(), "u8");
        assert_eq!(RustType::U16.render(), "u16");
        assert_eq!(RustType::U32.render(), "u32");
        assert_eq!(RustType::U64.render(), "u64");
        assert_eq!(RustType::U128.render(), "u128");
        assert_eq!(RustType::Isize.render(), "isize");
        assert_eq!(RustType::Usize.render(), "usize");
        assert_eq!(RustType::F32.render(), "f32");
        assert_eq!(RustType::F64.render(), "f64");
        assert_eq!(RustType::String.render(), "String");
        assert_eq!(RustType::VecU8.render(), "Vec<u8>");
    }

    #[test]
    fn test_render_composite_types() {
        assert_eq!(RustType::Vec(Box::new(RustType::I32)).render(), "Vec<i32>");
        assert_eq!(
            RustType::HashSet(Box::new(RustType::String)).render(),
            "HashSet<String>"
        );
        assert_eq!(
            RustType::HashMap(Box::new(RustType::String), Box::new(RustType::I32)).render(),
            "HashMap<String, i32>"
        );
        assert_eq!(
            RustType::Option(Box::new(RustType::Bool)).render(),
            "Option<bool>"
        );
        assert_eq!(
            RustType::Tuple(vec![RustType::I32, RustType::Bool]).render(),
            "(i32, bool)"
        );
    }

    #[test]
    fn test_render_nested_types() {
        let nested = RustType::Vec(Box::new(RustType::Option(Box::new(RustType::I32))));
        assert_eq!(nested.render(), "Vec<Option<i32>>");

        let deep = RustType::HashMap(
            Box::new(RustType::String),
            Box::new(RustType::Vec(Box::new(RustType::HashSet(Box::new(
                RustType::U64,
            ))))),
        );
        assert_eq!(deep.render(), "HashMap<String, Vec<HashSet<u64>>>");
    }

    #[test]
    fn test_is_hashable() {
        assert!(RustType::Bool.is_hashable());
        assert!(RustType::I32.is_hashable());
        assert!(RustType::U64.is_hashable());
        assert!(RustType::String.is_hashable());
        assert!(RustType::VecU8.is_hashable());
        assert!(RustType::Usize.is_hashable());
        assert!(RustType::Isize.is_hashable());

        assert!(!RustType::F32.is_hashable());
        assert!(!RustType::F64.is_hashable());
        assert!(!RustType::Vec(Box::new(RustType::I32)).is_hashable());
        assert!(!RustType::HashSet(Box::new(RustType::I32)).is_hashable());
        assert!(!RustType::HashMap(Box::new(RustType::I32), Box::new(RustType::I32)).is_hashable());

        // Option of hashable is hashable
        assert!(RustType::Option(Box::new(RustType::I32)).is_hashable());
        // Option of non-hashable is not
        assert!(!RustType::Option(Box::new(RustType::F32)).is_hashable());

        // Tuple of hashable is hashable
        assert!(RustType::Tuple(vec![RustType::I32, RustType::Bool]).is_hashable());
        // Tuple with float is not
        assert!(!RustType::Tuple(vec![RustType::I32, RustType::F64]).is_hashable());
    }

    #[test]
    fn test_is_integer() {
        assert!(RustType::I8.is_integer());
        assert!(RustType::I128.is_integer());
        assert!(RustType::U8.is_integer());
        assert!(RustType::U128.is_integer());
        assert!(RustType::Isize.is_integer());
        assert!(RustType::Usize.is_integer());

        assert!(!RustType::Bool.is_integer());
        assert!(!RustType::F32.is_integer());
        assert!(!RustType::String.is_integer());
        assert!(!RustType::Vec(Box::new(RustType::I32)).is_integer());
    }

    #[test]
    fn test_is_signed_int() {
        assert!(RustType::I8.is_signed_int());
        assert!(RustType::I16.is_signed_int());
        assert!(RustType::I32.is_signed_int());
        assert!(RustType::I64.is_signed_int());
        assert!(RustType::I128.is_signed_int());
        assert!(RustType::Isize.is_signed_int());

        assert!(!RustType::U8.is_signed_int());
        assert!(!RustType::U64.is_signed_int());
        assert!(!RustType::Usize.is_signed_int());
        assert!(!RustType::Bool.is_signed_int());
    }

    #[test]
    fn test_is_float() {
        assert!(RustType::F32.is_float());
        assert!(RustType::F64.is_float());

        assert!(!RustType::I32.is_float());
        assert!(!RustType::Bool.is_float());
        assert!(!RustType::String.is_float());
    }

    #[test]
    fn test_is_ord() {
        assert!(RustType::Bool.is_ord());
        assert!(RustType::I32.is_ord());
        assert!(RustType::String.is_ord());

        assert!(!RustType::F32.is_ord());
        assert!(!RustType::F64.is_ord());
        assert!(!RustType::HashSet(Box::new(RustType::I32)).is_ord());
        assert!(!RustType::HashMap(Box::new(RustType::I32), Box::new(RustType::I32)).is_ord());

        // Vec of ord is ord
        assert!(RustType::Vec(Box::new(RustType::I32)).is_ord());
        // Vec of float is not
        assert!(!RustType::Vec(Box::new(RustType::F32)).is_ord());

        // Option of ord is ord
        assert!(RustType::Option(Box::new(RustType::I32)).is_ord());

        // Tuple of ord is ord
        assert!(RustType::Tuple(vec![RustType::I32, RustType::Bool]).is_ord());
        assert!(!RustType::Tuple(vec![RustType::I32, RustType::F64]).is_ord());
    }

    #[test]
    fn test_is_eq() {
        assert!(RustType::Bool.is_eq());
        assert!(RustType::I32.is_eq());
        assert!(RustType::String.is_eq());

        assert!(!RustType::F32.is_eq());
        assert!(!RustType::F64.is_eq());

        // Vec of eq is eq
        assert!(RustType::Vec(Box::new(RustType::I32)).is_eq());
        assert!(!RustType::Vec(Box::new(RustType::F32)).is_eq());

        // HashSet of eq is eq
        assert!(RustType::HashSet(Box::new(RustType::I32)).is_eq());

        // HashMap needs both eq
        assert!(RustType::HashMap(Box::new(RustType::I32), Box::new(RustType::String)).is_eq());
        assert!(!RustType::HashMap(Box::new(RustType::I32), Box::new(RustType::F32)).is_eq());
    }

    #[test]
    fn test_is_collection() {
        assert!(RustType::Vec(Box::new(RustType::I32)).is_collection());
        assert!(RustType::VecU8.is_collection());
        assert!(RustType::HashSet(Box::new(RustType::I32)).is_collection());
        assert!(
            RustType::HashMap(Box::new(RustType::I32), Box::new(RustType::I32)).is_collection()
        );

        assert!(!RustType::Bool.is_collection());
        assert!(!RustType::String.is_collection());
        assert!(!RustType::Option(Box::new(RustType::I32)).is_collection());
    }

    #[test]
    fn test_is_string_like() {
        assert!(RustType::String.is_string_like());

        assert!(!RustType::Bool.is_string_like());
        assert!(!RustType::VecU8.is_string_like());
        assert!(!RustType::I32.is_string_like());
    }
}
