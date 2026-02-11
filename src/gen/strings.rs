use super::{BasicGenerator, Generate};
use crate::cbor_helpers::{cbor_map, map_insert};
use ciborium::Value;
use std::sync::OnceLock;

pub struct TextGenerator {
    min_size: usize,
    max_size: Option<usize>,
    cached_basic: OnceLock<Option<BasicGenerator<String>>>,
}

impl TextGenerator {
    pub fn with_min_size(mut self, min: usize) -> Self {
        self.min_size = min;
        self.cached_basic = OnceLock::new();
        self
    }

    pub fn with_max_size(mut self, max: usize) -> Self {
        self.max_size = Some(max);
        self.cached_basic = OnceLock::new();
        self
    }
}

impl Generate<String> for TextGenerator {
    fn generate(&self) -> String {
        self.as_basic().unwrap().generate()
    }

    fn as_basic(&self) -> Option<BasicGenerator<String>> {
        self.cached_basic
            .get_or_init(|| {
                let mut schema = cbor_map! {
                    "type" => "string",
                    "min_size" => self.min_size as u64
                };

                if let Some(max) = self.max_size {
                    map_insert(&mut schema, "max_size", Value::from(max as u64));
                }

                Some(BasicGenerator::new(schema))
            })
            .clone()
    }
}

pub fn text() -> TextGenerator {
    TextGenerator {
        min_size: 0,
        max_size: None,
        cached_basic: OnceLock::new(),
    }
}

pub struct RegexGenerator {
    pattern: String,
    fullmatch: bool,
    cached_basic: OnceLock<Option<BasicGenerator<String>>>,
}

impl RegexGenerator {
    /// Require the entire string to match the pattern, not just contain a match.
    pub fn fullmatch(mut self) -> Self {
        self.fullmatch = true;
        self.cached_basic = OnceLock::new();
        self
    }
}

impl Generate<String> for RegexGenerator {
    fn generate(&self) -> String {
        self.as_basic().unwrap().generate()
    }

    fn as_basic(&self) -> Option<BasicGenerator<String>> {
        self.cached_basic
            .get_or_init(|| {
                Some(BasicGenerator::new(cbor_map! {
                    "type" => "regex",
                    "pattern" => self.pattern.as_str(),
                    "fullmatch" => self.fullmatch
                }))
            })
            .clone()
    }
}

/// Generate strings that contain a match for the given regex pattern.
///
/// Use `.fullmatch()` to require the entire string to match.
pub fn from_regex(pattern: &str) -> RegexGenerator {
    RegexGenerator {
        pattern: pattern.to_string(),
        fullmatch: false,
        cached_basic: OnceLock::new(),
    }
}
