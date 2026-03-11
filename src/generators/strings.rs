use super::{BasicGenerator, Generator, TestCaseData};
use crate::cbor_utils::{cbor_array, cbor_map, map_insert};
use ciborium::Value;

pub struct TextGenerator {
    min_size: usize,
    max_size: Option<usize>,
}

impl TextGenerator {
    pub fn min_size(mut self, min_size: usize) -> Self {
        self.min_size = min_size;
        self
    }

    pub fn max_size(mut self, max_size: usize) -> Self {
        self.max_size = Some(max_size);
        self
    }

    fn build_schema(&self) -> Value {
        if let Some(max) = self.max_size {
            assert!(self.min_size <= max, "Cannot have max_size < min_size");
        }

        let mut schema = cbor_map! {
            "type" => "string",
            "min_size" => self.min_size as u64
        };

        if let Some(max) = self.max_size {
            map_insert(&mut schema, "max_size", max as u64);
        }

        schema
    }
}

impl Generator<String> for TextGenerator {
    fn do_draw(&self, data: &TestCaseData) -> String {
        data.generate_from_schema(&self.build_schema())
    }

    fn as_basic(&self) -> Option<BasicGenerator<'_, String>> {
        Some(BasicGenerator::new(self.build_schema(), |raw| {
            super::deserialize_value(raw)
        }))
    }
}

pub fn text() -> TextGenerator {
    TextGenerator {
        min_size: 0,
        max_size: None,
    }
}

pub struct RegexGenerator {
    pattern: String,
    fullmatch: bool,
}

impl RegexGenerator {
    /// Require the entire string to match the pattern, not just contain a match.
    pub fn fullmatch(mut self) -> Self {
        self.fullmatch = true;
        self
    }

    fn build_schema(&self) -> Value {
        cbor_map! {
            "type" => "regex",
            "pattern" => self.pattern.as_str(),
            "fullmatch" => self.fullmatch
        }
    }
}

impl Generator<String> for RegexGenerator {
    fn do_draw(&self, data: &TestCaseData) -> String {
        data.generate_from_schema(&self.build_schema())
    }

    fn as_basic(&self) -> Option<BasicGenerator<'_, String>> {
        Some(BasicGenerator::new(self.build_schema(), |raw| {
            super::deserialize_value(raw)
        }))
    }
}

/// Generate strings that contain a match for the given regex pattern.
///
/// Use `.fullmatch()` to require the entire string to match.
pub fn from_regex(pattern: &str) -> RegexGenerator {
    RegexGenerator {
        pattern: pattern.to_string(),
        fullmatch: false,
    }
}

pub struct BinaryGenerator {
    min_size: usize,
    max_size: Option<usize>,
}

impl BinaryGenerator {
    pub fn min_size(mut self, min_size: usize) -> Self {
        self.min_size = min_size;
        self
    }

    pub fn max_size(mut self, max_size: usize) -> Self {
        self.max_size = Some(max_size);
        self
    }

    fn build_schema(&self) -> Value {
        if let Some(max) = self.max_size {
            assert!(self.min_size <= max, "Cannot have max_size < min_size");
        }

        let mut schema = cbor_map! {
            "type" => "binary",
            "min_size" => self.min_size as u64
        };

        if let Some(max) = self.max_size {
            map_insert(&mut schema, "max_size", max as u64);
        }

        schema
    }
}

fn parse_binary(raw: Value) -> Vec<u8> {
    match raw {
        Value::Bytes(bytes) => bytes,
        _ => panic!("expected Value::Bytes, got {:?}", raw),
    }
}

impl Generator<Vec<u8>> for BinaryGenerator {
    fn do_draw(&self, data: &TestCaseData) -> Vec<u8> {
        parse_binary(data.generate_raw(&self.build_schema()))
    }

    fn as_basic(&self) -> Option<BasicGenerator<'_, Vec<u8>>> {
        Some(BasicGenerator::new(self.build_schema(), parse_binary))
    }
}

/// Generate binary data.
///
/// # Example
///
/// ```no_run
/// use hegel::generators::{self, Generator};
///
/// // Generate any byte sequence
/// let gen = generators::binary();
///
/// // Generate 16-32 bytes
/// let gen = generators::binary().min_size(16).max_size(32);
/// ```
pub fn binary() -> BinaryGenerator {
    BinaryGenerator {
        min_size: 0,
        max_size: None,
    }
}

pub struct EmailGenerator;

impl Generator<String> for EmailGenerator {
    fn do_draw(&self, data: &TestCaseData) -> String {
        data.generate_from_schema(&cbor_map! {"type" => "email"})
    }

    fn as_basic(&self) -> Option<BasicGenerator<'_, String>> {
        Some(BasicGenerator::new(cbor_map! {"type" => "email"}, |raw| {
            super::deserialize_value(raw)
        }))
    }
}

pub fn emails() -> EmailGenerator {
    EmailGenerator
}

pub struct UrlGenerator;

impl Generator<String> for UrlGenerator {
    fn do_draw(&self, data: &TestCaseData) -> String {
        data.generate_from_schema(&cbor_map! {"type" => "url"})
    }

    fn as_basic(&self) -> Option<BasicGenerator<'_, String>> {
        Some(BasicGenerator::new(cbor_map! {"type" => "url"}, |raw| {
            super::deserialize_value(raw)
        }))
    }
}

pub fn urls() -> UrlGenerator {
    UrlGenerator
}

pub struct DomainGenerator {
    max_length: usize,
}

impl DomainGenerator {
    pub fn max_length(mut self, max_length: usize) -> Self {
        self.max_length = max_length;
        self
    }

    fn build_schema(&self) -> Value {
        assert!(
            self.max_length >= 4 && self.max_length <= 255,
            "max_length must be between 4 and 255"
        );

        cbor_map! {
            "type" => "domain",
            "max_length" => self.max_length as u64
        }
    }
}

impl Generator<String> for DomainGenerator {
    fn do_draw(&self, data: &TestCaseData) -> String {
        data.generate_from_schema(&self.build_schema())
    }

    fn as_basic(&self) -> Option<BasicGenerator<'_, String>> {
        Some(BasicGenerator::new(self.build_schema(), |raw| {
            super::deserialize_value(raw)
        }))
    }
}

pub fn domains() -> DomainGenerator {
    DomainGenerator { max_length: 255 }
}

#[derive(Clone, Copy)]
pub enum IpVersion {
    V4,
    V6,
}

pub struct IpAddressGenerator {
    version: Option<IpVersion>,
}

impl IpAddressGenerator {
    pub fn v4(mut self) -> Self {
        self.version = Some(IpVersion::V4);
        self
    }

    pub fn v6(mut self) -> Self {
        self.version = Some(IpVersion::V6);
        self
    }

    fn build_schema(&self) -> Value {
        match self.version {
            Some(IpVersion::V4) => cbor_map! {"type" => "ipv4"},
            Some(IpVersion::V6) => cbor_map! {"type" => "ipv6"},
            None => cbor_map! {
                "one_of" => cbor_array![
                    cbor_map!{"type" => "ipv4"},
                    cbor_map!{"type" => "ipv6"}
                ]
            },
        }
    }
}

impl Generator<String> for IpAddressGenerator {
    fn do_draw(&self, data: &TestCaseData) -> String {
        data.generate_from_schema(&self.build_schema())
    }

    fn as_basic(&self) -> Option<BasicGenerator<'_, String>> {
        Some(BasicGenerator::new(self.build_schema(), |raw| {
            super::deserialize_value(raw)
        }))
    }
}

pub fn ip_addresses() -> IpAddressGenerator {
    IpAddressGenerator { version: None }
}

pub struct DateGenerator;

impl Generator<String> for DateGenerator {
    fn do_draw(&self, data: &TestCaseData) -> String {
        data.generate_from_schema(&cbor_map! {"type" => "date"})
    }

    fn as_basic(&self) -> Option<BasicGenerator<'_, String>> {
        Some(BasicGenerator::new(cbor_map! {"type" => "date"}, |raw| {
            super::deserialize_value(raw)
        }))
    }
}

pub fn dates() -> DateGenerator {
    DateGenerator
}

pub struct TimeGenerator;

impl Generator<String> for TimeGenerator {
    fn do_draw(&self, data: &TestCaseData) -> String {
        data.generate_from_schema(&cbor_map! {"type" => "time"})
    }

    fn as_basic(&self) -> Option<BasicGenerator<'_, String>> {
        Some(BasicGenerator::new(cbor_map! {"type" => "time"}, |raw| {
            super::deserialize_value(raw)
        }))
    }
}

pub fn times() -> TimeGenerator {
    TimeGenerator
}

pub struct DateTimeGenerator;

impl Generator<String> for DateTimeGenerator {
    fn do_draw(&self, data: &TestCaseData) -> String {
        data.generate_from_schema(&cbor_map! {"type" => "datetime"})
    }

    fn as_basic(&self) -> Option<BasicGenerator<'_, String>> {
        Some(BasicGenerator::new(
            cbor_map! {"type" => "datetime"},
            super::deserialize_value,
        ))
    }
}

pub fn datetimes() -> DateTimeGenerator {
    DateTimeGenerator
}
