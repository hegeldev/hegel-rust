use super::{BasicGenerator, Generator, TestCase};
use crate::cbor_utils::{cbor_array, cbor_map, map_insert};
use ciborium::Value;

/// Generator for single Unicode characters. Created by [`characters()`].
pub struct CharactersGenerator {
    codec: Option<String>,
    min_codepoint: Option<u32>,
    max_codepoint: Option<u32>,
    categories: Option<Vec<String>>,
    exclude_categories: Option<Vec<String>>,
    include_characters: Option<String>,
    exclude_characters: Option<String>,
}

impl CharactersGenerator {
    /// Restrict to this codec (e.g. `"ascii"`, `"utf-8"`).
    pub fn codec(mut self, codec: &str) -> Self {
        self.codec = Some(codec.to_string());
        self
    }

    /// Set the minimum Unicode codepoint.
    pub fn min_codepoint(mut self, cp: u32) -> Self {
        self.min_codepoint = Some(cp);
        self
    }

    /// Set the maximum Unicode codepoint.
    pub fn max_codepoint(mut self, cp: u32) -> Self {
        self.max_codepoint = Some(cp);
        self
    }

    /// Include only characters from these Unicode categories (e.g. `["L", "Nd"]`).
    /// Mutually exclusive with [`exclude_categories`](Self::exclude_categories).
    pub fn categories(mut self, cats: &[&str]) -> Self {
        self.categories = Some(cats.iter().map(|s| s.to_string()).collect());
        self.exclude_categories = None;
        self
    }

    /// Exclude characters from these Unicode categories.
    /// Mutually exclusive with [`categories`](Self::categories).
    ///
    /// The `Cs` (surrogate) category is always excluded because Rust strings
    /// must be valid UTF-8 and cannot represent surrogate code points.
    pub fn exclude_categories(mut self, cats: &[&str]) -> Self {
        let mut all: Vec<String> = cats.iter().map(|s| s.to_string()).collect();
        if !all.iter().any(|c| c == "Cs") {
            all.push("Cs".to_string());
        }
        self.exclude_categories = Some(all);
        self.categories = None;
        self
    }

    /// Always include these specific characters, even if excluded by other filters.
    pub fn include_characters(mut self, chars: &str) -> Self {
        self.include_characters = Some(chars.to_string());
        self
    }

    /// Always exclude these specific characters.
    pub fn exclude_characters(mut self, chars: &str) -> Self {
        self.exclude_characters = Some(chars.to_string());
        self
    }

    fn insert_into_schema(&self, schema: &mut Value) {
        if let Some(ref codec) = self.codec {
            map_insert(schema, "codec", codec.as_str());
        }
        if let Some(codepoint) = self.min_codepoint {
            map_insert(schema, "min_codepoint", codepoint as u64);
        }
        if let Some(codepoint) = self.max_codepoint {
            map_insert(schema, "max_codepoint", codepoint as u64);
        }
        if let Some(ref cats) = self.categories {
            map_insert(
                schema,
                "categories",
                Value::Array(cats.iter().map(|s| Value::from(s.as_str())).collect()),
            );
        }
        if let Some(ref cats) = self.exclude_categories {
            map_insert(
                schema,
                "exclude_categories",
                Value::Array(cats.iter().map(|s| Value::from(s.as_str())).collect()),
            );
        }
        if let Some(ref chars) = self.include_characters {
            map_insert(schema, "include_characters", chars.as_str());
        }
        if let Some(ref chars) = self.exclude_characters {
            map_insert(schema, "exclude_characters", chars.as_str());
        }
    }

    fn build_alphabet_schema(&self) -> Value {
        let mut schema = cbor_map! {};
        self.insert_into_schema(&mut schema);
        schema
    }
}

impl Generator<String> for CharactersGenerator {
    fn do_draw(&self, tc: &TestCase) -> String {
        let mut schema = cbor_map! {
            "type" => "string",
            "min_size" => 1u64,
            "max_size" => 1u64
        };
        self.insert_into_schema(&mut schema);
        super::generate_from_schema(tc, &schema)
    }

    fn as_basic(&self) -> Option<BasicGenerator<'_, String>> {
        let mut schema = cbor_map! {
            "type" => "string",
            "min_size" => 1u64,
            "max_size" => 1u64
        };
        self.insert_into_schema(&mut schema);
        Some(BasicGenerator::new(schema, super::deserialize_value))
    }
}

/// Generate single Unicode characters.
///
/// By default, surrogates (Unicode category `Cs`) are excluded because Rust's
/// `char` type cannot represent them. Other client libraries (e.g. TypeScript,
/// Python) may include surrogates by default.
pub fn characters() -> CharactersGenerator {
    CharactersGenerator {
        codec: None,
        min_codepoint: None,
        max_codepoint: None,
        categories: None,
        exclude_categories: Some(vec!["Cs".to_string()]),
        include_characters: None,
        exclude_characters: None,
    }
}

/// Generator for Unicode text strings. Created by [`text()`].
pub struct TextGenerator {
    min_size: usize,
    max_size: Option<usize>,
    characters: CharactersGenerator,
    alphabet_called: bool,
    character_param_called: bool,
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

    /// Use a fixed set of characters. Each character in the string is a member
    /// of the alphabet.
    ///
    /// Mutually exclusive with the character filtering methods like `codec`,
    /// `categories`, `min_codepoint`, etc.
    pub fn alphabet(mut self, chars: &str) -> Self {
        self.characters = CharactersGenerator {
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
        self.character_param_called = true;
        self.characters = self.characters.codec(codec);
        self
    }

    /// Set the minimum Unicode codepoint.
    pub fn min_codepoint(mut self, cp: u32) -> Self {
        self.character_param_called = true;
        self.characters = self.characters.min_codepoint(cp);
        self
    }

    /// Set the maximum Unicode codepoint.
    pub fn max_codepoint(mut self, cp: u32) -> Self {
        self.character_param_called = true;
        self.characters = self.characters.max_codepoint(cp);
        self
    }

    /// Include only characters from these Unicode categories (e.g. `["L", "Nd"]`).
    /// Mutually exclusive with [`exclude_categories`](Self::exclude_categories).
    pub fn categories(mut self, cats: &[&str]) -> Self {
        self.character_param_called = true;
        self.characters = self.characters.categories(cats);
        self
    }

    /// Exclude characters from these Unicode categories.
    /// Mutually exclusive with [`categories`](Self::categories).
    pub fn exclude_categories(mut self, cats: &[&str]) -> Self {
        self.character_param_called = true;
        self.characters = self.characters.exclude_categories(cats);
        self
    }

    /// Always include these specific characters, even if excluded by other filters.
    pub fn include_characters(mut self, chars: &str) -> Self {
        self.character_param_called = true;
        self.characters = self.characters.include_characters(chars);
        self
    }

    /// Always exclude these specific characters.
    pub fn exclude_characters(mut self, chars: &str) -> Self {
        self.character_param_called = true;
        self.characters = self.characters.exclude_characters(chars);
        self
    }

    fn build_schema(&self) -> Value {
        assert!(
            !(self.alphabet_called && self.character_param_called),
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

        self.characters.insert_into_schema(&mut schema);

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
pub fn text() -> TextGenerator {
    TextGenerator {
        min_size: 0,
        max_size: None,
        characters: characters(),
        alphabet_called: false,
        character_param_called: false,
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
    // nocov start
    pub fn fullmatch(mut self, fullmatch: bool) -> Self {
        self.fullmatch = fullmatch;
        self
        // nocov end
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
            Some(IpVersion::V4) => cbor_map! {"type" => "ipv4"},
            Some(IpVersion::V6) => cbor_map! {"type" => "ipv6"},
            None => cbor_map! {
                "type" => "one_of",
                "generators" => cbor_array![
                    cbor_map!{"type" => "ipv4"},
                    cbor_map!{"type" => "ipv6"}
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
