use super::{BasicGenerator, Generator, TestCase};
use crate::cbor_utils::{cbor_array, cbor_map, map_extend, map_insert};
use ciborium::Value;

/// Categories that include surrogate codepoints. Rust strings cannot contain
/// surrogates, so these are forbidden in `categories()`.
const SURROGATE_CATEGORIES: &[&str] = &["Cs", "C"];

/// Shared character filtering fields used by both [`TextGenerator`] and
/// [`CharactersGenerator`].
struct CharacterFields {
    codec: Option<String>,
    min_codepoint: Option<u32>,
    max_codepoint: Option<u32>,
    categories: Option<Vec<String>>,
    exclude_categories: Option<Vec<String>>,
    include_characters: Option<String>,
    exclude_characters: Option<String>,
}

impl CharacterFields {
    fn new() -> Self {
        CharacterFields {
            codec: None,
            min_codepoint: None,
            max_codepoint: None,
            categories: None,
            exclude_categories: None,
            include_characters: None,
            exclude_characters: None,
        }
    }

    /// Build a schema map containing the character filtering fields.
    fn to_schema(&self) -> Value {
        let mut schema = cbor_map! {};
        if let Some(ref codec) = self.codec {
            map_insert(&mut schema, "codec", codec.as_str());
        }
        if let Some(min_cp) = self.min_codepoint {
            map_insert(&mut schema, "min_codepoint", min_cp as u64);
        }
        if let Some(max_cp) = self.max_codepoint {
            map_insert(&mut schema, "max_codepoint", max_cp as u64);
        }
        if let Some(ref cats) = self.categories {
            for cat in cats {
                assert!(
                    !SURROGATE_CATEGORIES.contains(&cat.as_str()),
                    "Category \"{cat}\" includes surrogate codepoints (Cs), \
                     which Rust strings cannot represent."
                );
            }
            let arr = Value::Array(cats.iter().map(|c| Value::from(c.as_str())).collect());
            map_insert(&mut schema, "categories", arr);
        } else {
            // Always exclude surrogates (Cs) since Rust strings cannot contain them.
            let mut excl = self.exclude_categories.clone().unwrap_or_default();
            if !excl.iter().any(|c| c == "Cs") {
                excl.push("Cs".to_string());
            }
            let arr = Value::Array(excl.iter().map(|c| Value::from(c.as_str())).collect());
            map_insert(&mut schema, "exclude_categories", arr);
        }
        if let Some(ref incl) = self.include_characters {
            map_insert(&mut schema, "include_characters", incl.as_str());
        }
        if let Some(ref excl) = self.exclude_characters {
            map_insert(&mut schema, "exclude_characters", excl.as_str());
        }
        schema
    }
}

/// Generator for Unicode text strings. Created by [`text()`].
pub struct TextGenerator {
    min_size: usize,
    max_size: Option<usize>,
    char_fields: CharacterFields,
    alphabet_called: bool,
    char_param_called: bool,
}

impl TextGenerator {
    /// Set the minimum length in characters.
    pub fn min_size(mut self, min_size: usize) -> Self {
        self.min_size = min_size;
        self
    }

    /// Set the maximum length in characters.
    pub fn max_size(mut self, max_size: usize) -> Self {
        self.max_size = Some(max_size);
        self
    }

    /// Use a fixed set of characters. Each character in the generated string
    /// will be a member of the alphabet.
    ///
    /// Mutually exclusive with the character filtering methods like `codec`,
    /// `categories`, `min_codepoint`, etc.
    pub fn alphabet(mut self, chars: &str) -> Self {
        self.char_fields = CharacterFields {
            codec: None,
            min_codepoint: None,
            max_codepoint: None,
            categories: Some(vec![]),
            exclude_categories: None,
            include_characters: Some(chars.to_string()),
            exclude_characters: None,
        };
        self.alphabet_called = true;
        self
    }

    /// Restrict to characters encodable in this codec (e.g. `"ascii"`, `"utf-8"`, `"latin-1"`).
    pub fn codec(mut self, codec: &str) -> Self {
        self.char_param_called = true;
        self.char_fields.codec = Some(codec.to_string());
        self
    }

    /// Set the minimum Unicode codepoint.
    pub fn min_codepoint(mut self, min_codepoint: u32) -> Self {
        self.char_param_called = true;
        self.char_fields.min_codepoint = Some(min_codepoint);
        self
    }

    /// Set the maximum Unicode codepoint.
    pub fn max_codepoint(mut self, max_codepoint: u32) -> Self {
        self.char_param_called = true;
        self.char_fields.max_codepoint = Some(max_codepoint);
        self
    }

    /// Include only characters from these Unicode general categories (e.g. `["L", "Nd"]`).
    ///
    /// Mutually exclusive with [`exclude_categories`](Self::exclude_categories).
    pub fn categories(mut self, categories: &[&str]) -> Self {
        self.char_param_called = true;
        self.char_fields.categories = Some(categories.iter().map(|s| s.to_string()).collect());
        self
    }

    /// Exclude characters from these Unicode general categories.
    ///
    /// Mutually exclusive with [`categories`](Self::categories).
    pub fn exclude_categories(mut self, exclude_categories: &[&str]) -> Self {
        self.char_param_called = true;
        self.char_fields.exclude_categories =
            Some(exclude_categories.iter().map(|s| s.to_string()).collect());
        self
    }

    /// Always include these specific characters, even if excluded by other filters.
    pub fn include_characters(mut self, include_characters: &str) -> Self {
        self.char_param_called = true;
        self.char_fields.include_characters = Some(include_characters.to_string());
        self
    }

    /// Always exclude these specific characters.
    pub fn exclude_characters(mut self, exclude_characters: &str) -> Self {
        self.char_param_called = true;
        self.char_fields.exclude_characters = Some(exclude_characters.to_string());
        self
    }

    fn build_schema(&self) -> Value {
        assert!(
            !(self.alphabet_called && self.char_param_called),
            "Cannot combine .alphabet() with character methods."
        );
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
        map_extend(&mut schema, self.char_fields.to_schema());

        schema
    }
}

impl Generator<String> for TextGenerator {
    fn do_draw(&self, tc: &TestCase) -> String {
        super::generate_from_schema(tc, &self.build_schema())
    }

    fn as_basic(&self) -> Option<BasicGenerator<'_, String>> {
        Some(BasicGenerator::new(
            self.build_schema(),
            super::deserialize_value,
        ))
    }
}

/// Generate arbitrary Unicode text strings.
///
/// See [`TextGenerator`] for builder methods.
pub fn text() -> TextGenerator {
    TextGenerator {
        min_size: 0,
        max_size: None,
        char_fields: CharacterFields::new(),
        alphabet_called: false,
        char_param_called: false,
    }
}

/// Generator for single Unicode characters ([`char`]). Created by [`characters()`].
pub struct CharactersGenerator {
    char_fields: CharacterFields,
}

impl CharactersGenerator {
    /// Restrict to characters encodable in this codec (e.g. `"ascii"`, `"utf-8"`, `"latin-1"`).
    pub fn codec(mut self, codec: &str) -> Self {
        self.char_fields.codec = Some(codec.to_string());
        self
    }

    /// Set the minimum Unicode codepoint.
    pub fn min_codepoint(mut self, min_codepoint: u32) -> Self {
        self.char_fields.min_codepoint = Some(min_codepoint);
        self
    }

    /// Set the maximum Unicode codepoint.
    pub fn max_codepoint(mut self, max_codepoint: u32) -> Self {
        self.char_fields.max_codepoint = Some(max_codepoint);
        self
    }

    /// Include only characters from these Unicode general categories (e.g. `["L", "Nd"]`).
    ///
    /// Mutually exclusive with [`exclude_categories`](Self::exclude_categories).
    pub fn categories(mut self, categories: &[&str]) -> Self {
        self.char_fields.categories = Some(categories.iter().map(|s| s.to_string()).collect());
        self
    }

    /// Exclude characters from these Unicode general categories.
    ///
    /// Mutually exclusive with [`categories`](Self::categories).
    pub fn exclude_categories(mut self, exclude_categories: &[&str]) -> Self {
        self.char_fields.exclude_categories =
            Some(exclude_categories.iter().map(|s| s.to_string()).collect());
        self
    }

    /// Always include these specific characters, even if excluded by other filters.
    pub fn include_characters(mut self, include_characters: &str) -> Self {
        self.char_fields.include_characters = Some(include_characters.to_string());
        self
    }

    /// Always exclude these specific characters.
    pub fn exclude_characters(mut self, exclude_characters: &str) -> Self {
        self.char_fields.exclude_characters = Some(exclude_characters.to_string());
        self
    }

    fn build_schema(&self) -> Value {
        let mut schema = cbor_map! {
            "type" => "string",
            "min_size" => 1u64,
            "max_size" => 1u64
        };
        map_extend(&mut schema, self.char_fields.to_schema());
        schema
    }

    /// Build a standalone schema for use as a regex alphabet constraint.
    pub(super) fn build_alphabet_schema(&self) -> Value {
        self.char_fields.to_schema()
    }
}

fn parse_char(raw: Value) -> char {
    let s: String = super::deserialize_value(raw);
    let mut chars = s.chars();
    let c = chars
        .next()
        .expect("expected a single character, got empty string");
    assert!(
        chars.next().is_none(),
        "expected a single character, got multiple"
    );
    c
}

impl Generator<char> for CharactersGenerator {
    fn do_draw(&self, tc: &TestCase) -> char {
        parse_char(super::generate_raw(tc, &self.build_schema()))
    }

    fn as_basic(&self) -> Option<BasicGenerator<'_, char>> {
        Some(BasicGenerator::new(self.build_schema(), parse_char))
    }
}

/// Generate single Unicode characters ([`char`]).
///
/// See [`CharactersGenerator`] for builder methods.
pub fn characters() -> CharactersGenerator {
    CharactersGenerator {
        char_fields: CharacterFields::new(),
    }
}

/// Generator for strings matching a regex pattern. Created by [`from_regex()`].
///
/// By default generates strings that contain a match. Use [`fullmatch()`](Self::fullmatch)
/// to require the entire string to match.
pub struct RegexGenerator {
    pattern: String,
    fullmatch: bool,
    alphabet: Option<CharactersGenerator>,
}

impl RegexGenerator {
    /// Set whether the entire string must match the pattern, not just contain a match.
    pub fn fullmatch(mut self, fullmatch: bool) -> Self {
        self.fullmatch = fullmatch;
        self
    }

    /// Constrain which characters may appear in generated strings.
    pub fn alphabet(mut self, alphabet: CharactersGenerator) -> Self {
        self.alphabet = Some(alphabet);
        self
    }

    // nocov start
    fn build_schema(&self) -> Value {
        let mut schema = cbor_map! {
            "type" => "regex",
            "pattern" => self.pattern.as_str(),
            "fullmatch" => self.fullmatch
        // nocov end
        };

        if let Some(ref alphabet) = self.alphabet {
            map_insert(&mut schema, "alphabet", alphabet.build_alphabet_schema());
        }

        schema
    }
}

impl Generator<String> for RegexGenerator {
    // nocov start
    fn do_draw(&self, tc: &TestCase) -> String {
        super::generate_from_schema(tc, &self.build_schema())
        // nocov end
    }

    // nocov start
    fn as_basic(&self) -> Option<BasicGenerator<'_, String>> {
        Some(BasicGenerator::new(
            self.build_schema(),
            super::deserialize_value,
            // nocov end
        ))
    }
}

/// Generate strings matching a regex pattern.
///
/// See [`RegexGenerator`] for builder methods.
// nocov start
pub fn from_regex(pattern: &str) -> RegexGenerator {
    RegexGenerator {
        pattern: pattern.to_string(),
        fullmatch: false,
        alphabet: None,
        // nocov end
    }
}

/// Generator for arbitrary byte sequences. Created by [`binary()`].
pub struct BinaryGenerator {
    min_size: usize,
    max_size: Option<usize>,
}

impl BinaryGenerator {
    /// Set the minimum length in bytes.
    pub fn min_size(mut self, min_size: usize) -> Self {
        self.min_size = min_size;
        self
    }

    /// Set the maximum length in bytes.
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
        _ => panic!("expected Value::Bytes, got {:?}", raw), // nocov
    }
}

impl Generator<Vec<u8>> for BinaryGenerator {
    fn do_draw(&self, tc: &TestCase) -> Vec<u8> {
        parse_binary(super::generate_raw(tc, &self.build_schema()))
    }

    fn as_basic(&self) -> Option<BasicGenerator<'_, Vec<u8>>> {
        Some(BasicGenerator::new(self.build_schema(), parse_binary))
    }
}

/// Generate arbitrary byte sequences (`Vec<u8>`).
///
/// See [`BinaryGenerator`] for builder methods.
pub fn binary() -> BinaryGenerator {
    BinaryGenerator {
        min_size: 0,
        max_size: None,
    }
}

/// Generator for email address strings. Created by [`emails()`].
pub struct EmailGenerator;

impl Generator<String> for EmailGenerator {
    // nocov start
    fn do_draw(&self, tc: &TestCase) -> String {
        super::generate_from_schema(tc, &cbor_map! {"type" => "email"})
        // nocov end
    }

    // nocov start
    fn as_basic(&self) -> Option<BasicGenerator<'_, String>> {
        Some(BasicGenerator::new(cbor_map! {"type" => "email"}, |raw| {
            super::deserialize_value(raw)
            // nocov end
        }))
    }
}

/// Generate email address strings.
// nocov start
pub fn emails() -> EmailGenerator {
    EmailGenerator
    // nocov end
}

/// Generator for URL strings. Created by [`urls()`].
pub struct UrlGenerator;

impl Generator<String> for UrlGenerator {
    // nocov start
    fn do_draw(&self, tc: &TestCase) -> String {
        super::generate_from_schema(tc, &cbor_map! {"type" => "url"})
        // nocov end
    }

    // nocov start
    fn as_basic(&self) -> Option<BasicGenerator<'_, String>> {
        Some(BasicGenerator::new(cbor_map! {"type" => "url"}, |raw| {
            super::deserialize_value(raw)
            // nocov end
        }))
    }
}

/// Generate URL strings.
// nocov start
pub fn urls() -> UrlGenerator {
    UrlGenerator
    // nocov end
}

/// Generator for domain name strings. Created by [`domains()`].
pub struct DomainGenerator {
    max_length: usize,
}

impl DomainGenerator {
    /// Set the maximum length (must be between 4 and 255).
    pub fn max_length(mut self, max_length: usize) -> Self {
        self.max_length = max_length;
        self
    }

    fn build_schema(&self) -> Value {
        assert!(
            self.max_length >= 4 && self.max_length <= 255,
            "max_length must be between 4 and 255"
        );

        cbor_map! { // nocov
            "type" => "domain",
            "max_length" => self.max_length as u64 // nocov
        }
    }
}

impl Generator<String> for DomainGenerator {
    // nocov start
    fn do_draw(&self, tc: &TestCase) -> String {
        super::generate_from_schema(tc, &self.build_schema())
        // nocov end
    }

    fn as_basic(&self) -> Option<BasicGenerator<'_, String>> {
        Some(BasicGenerator::new(self.build_schema(), |raw| {
            super::deserialize_value(raw) // nocov
        }))
    }
}

/// Generate domain name strings.
///
/// See [`DomainGenerator`] for builder methods.
pub fn domains() -> DomainGenerator {
    DomainGenerator { max_length: 255 }
}

#[derive(Clone, Copy)]
pub enum IpVersion {
    V4,
    V6,
}

/// Generator for IP address strings. Created by [`ip_addresses()`].
///
/// By default generates both IPv4 and IPv6 addresses.
pub struct IpAddressGenerator {
    version: Option<IpVersion>,
}

impl IpAddressGenerator {
    /// Only generate IPv4 addresses.
    // nocov start
    pub fn v4(mut self) -> Self {
        self.version = Some(IpVersion::V4);
        self
        // nocov end
    }

    /// Only generate IPv6 addresses.
    // nocov start
    pub fn v6(mut self) -> Self {
        self.version = Some(IpVersion::V6);
        self
        // nocov end
    }

    // nocov start
    fn build_schema(&self) -> Value {
        match self.version {
            Some(IpVersion::V4) => cbor_map! {"type" => "ip_addresses", "version" => 4u64},
            Some(IpVersion::V6) => cbor_map! {"type" => "ip_addresses", "version" => 6u64},
            None => cbor_map! {
                "type" => "one_of",
                "generators" => cbor_array![
                    cbor_map!{"type" => "ip_addresses", "version" => 4u64},
                    cbor_map!{"type" => "ip_addresses", "version" => 6u64}
            // nocov end
                ]
            },
        }
    }
}

impl Generator<String> for IpAddressGenerator {
    // nocov start
    fn do_draw(&self, tc: &TestCase) -> String {
        super::generate_from_schema(tc, &self.build_schema())
        // nocov end
    }

    // nocov start
    fn as_basic(&self) -> Option<BasicGenerator<'_, String>> {
        Some(BasicGenerator::new(self.build_schema(), |raw| {
            super::deserialize_value(raw)
            // nocov end
        }))
    }
}

/// Generate IP address strings (IPv4 or IPv6).
///
/// See [`IpAddressGenerator`] for builder methods.
// nocov start
pub fn ip_addresses() -> IpAddressGenerator {
    IpAddressGenerator { version: None }
    // nocov end
}

/// Generator for date strings in YYYY-MM-DD format. Created by [`dates()`].
pub struct DateGenerator;

impl Generator<String> for DateGenerator {
    // nocov start
    fn do_draw(&self, tc: &TestCase) -> String {
        super::generate_from_schema(tc, &cbor_map! {"type" => "date"})
        // nocov end
    }

    // nocov start
    fn as_basic(&self) -> Option<BasicGenerator<'_, String>> {
        Some(BasicGenerator::new(cbor_map! {"type" => "date"}, |raw| {
            super::deserialize_value(raw)
            // nocov end
        }))
    }
}

/// Generate date strings in YYYY-MM-DD format.
// nocov start
pub fn dates() -> DateGenerator {
    DateGenerator
    // nocov end
}

/// Generator for time strings in HH:MM:SS format. Created by [`times()`].
pub struct TimeGenerator;

impl Generator<String> for TimeGenerator {
    // nocov start
    fn do_draw(&self, tc: &TestCase) -> String {
        super::generate_from_schema(tc, &cbor_map! {"type" => "time"})
        // nocov end
    }

    // nocov start
    fn as_basic(&self) -> Option<BasicGenerator<'_, String>> {
        Some(BasicGenerator::new(cbor_map! {"type" => "time"}, |raw| {
            super::deserialize_value(raw)
            // nocov end
        }))
    }
}

/// Generate time strings in HH:MM:SS format.
// nocov start
pub fn times() -> TimeGenerator {
    TimeGenerator
    // nocov end
}

/// Generator for ISO 8601 datetime strings. Created by [`datetimes()`].
pub struct DateTimeGenerator;

impl Generator<String> for DateTimeGenerator {
    // nocov start
    fn do_draw(&self, tc: &TestCase) -> String {
        super::generate_from_schema(tc, &cbor_map! {"type" => "datetime"})
        // nocov end
    }

    // nocov start
    fn as_basic(&self) -> Option<BasicGenerator<'_, String>> {
        Some(BasicGenerator::new(
            cbor_map! {"type" => "datetime"},
            super::deserialize_value,
            // nocov end
        ))
    }
}

/// Generate ISO 8601 datetime strings.
// nocov start
pub fn datetimes() -> DateTimeGenerator {
    DateTimeGenerator
    // nocov end
}
