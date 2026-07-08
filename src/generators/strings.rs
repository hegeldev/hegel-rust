use std::sync::OnceLock;

use super::generators::draw_and_print_value;
use super::{Generator, PrintableGenerator, TestCase, labels};
use crate::control::hegel_internal_assert;
use crate::ffi;
use crate::pretty::PrettyPrinter;
use crate::test_case::{full_ranges, invalid_argument};

/// Categories that include surrogate codepoints. Rust strings cannot contain
/// surrogates, so these are forbidden in `categories()`.
const SURROGATE_CATEGORIES: &[&str] = &["Cs", "C"];

/// Default upper bound for string/byte sizes when the caller doesn't set one.
const DEFAULT_MAX_SIZE: usize = 100;

/// Codec names accepted by [`TextGenerator::codec`] and
/// [`CharactersGenerator::codec`], mirroring the engine's supported set.
const SUPPORTED_CODECS: &[&str] = &["ascii", "latin-1", "iso-8859-1", "utf-8"];

/// Validate a codec name eagerly. The engine rejects unknown codecs too, but
/// only when the alphabet is built on first draw; checking here surfaces the
/// mistake at the `.codec(...)` call site instead.
fn check_codec(codec: &str) {
    if !SUPPORTED_CODECS.contains(&codec) {
        invalid_argument!("invalid codec: {codec}");
    }
}

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

    /// Build a libhegel text generator over this alphabet with the given
    /// size bounds. Surrogates are always excluded: category restrictions
    /// naming a surrogate category are rejected, and without explicit
    /// categories the `Cs` category is excluded (Rust strings cannot hold
    /// surrogates).
    fn build_text_handle(&self, min_size: u64, max_size: u64) -> ffi::StringGenerator {
        let (categories, exclude_categories) = if let Some(ref cats) = self.categories {
            for cat in cats {
                if SURROGATE_CATEGORIES.contains(&cat.as_str()) {
                    invalid_argument!(
                        "Category \"{cat}\" includes surrogate codepoints (Cs), \
                         which Rust strings cannot represent."
                    );
                }
            }
            (Some(cats.clone()), None)
        } else {
            let mut excl = self.exclude_categories.clone().unwrap_or_default();
            if !excl.iter().any(|c| c == "Cs") {
                excl.push("Cs".to_string());
            }
            (None, Some(excl))
        };
        ffi::StringGenerator::text(
            min_size,
            max_size,
            self.codec.as_deref(),
            self.min_codepoint.unwrap_or(0),
            self.max_codepoint,
            categories.as_deref(),
            exclude_categories.as_deref(),
            self.include_characters.as_deref(),
            self.exclude_characters.as_deref(),
        )
        .unwrap_or_else(|msg| invalid_argument!("{msg}"))
    }
}

/// Generator for Unicode text strings. Created by [`text()`].
pub struct TextGenerator {
    min_size: usize,
    max_size: Option<usize>,
    char_fields: CharacterFields,
    alphabet_called: bool,
    char_param_called: bool,
    handle: OnceLock<ffi::StringGenerator>,
}

impl TextGenerator {
    /// Set the minimum length in characters.
    pub fn min_size(mut self, min_size: usize) -> Self {
        self.handle = OnceLock::new();
        self.min_size = min_size;
        self
    }

    /// Set the maximum length in characters.
    pub fn max_size(mut self, max_size: usize) -> Self {
        self.handle = OnceLock::new();
        self.max_size = Some(max_size);
        self
    }

    /// Use a fixed set of characters. Each character in the generated string
    /// will be a member of the alphabet.
    ///
    /// Mutually exclusive with the character filtering methods like `codec`,
    /// `categories`, `min_codepoint`, etc.
    pub fn alphabet(mut self, chars: &str) -> Self {
        self.handle = OnceLock::new();
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

    /// Restrict to characters encodable in the named codec.
    ///
    /// Supported values, and what each means for Rust strings:
    ///
    /// - `"ascii"` — codepoints `U+0000..=U+007F`; equivalent to
    ///   `.max_codepoint(0x7F)`.
    /// - `"latin-1"` (alias `"iso-8859-1"`) — codepoints `U+0000..=U+00FF`;
    ///   equivalent to `.max_codepoint(0xFF)`.
    /// - `"utf-8"` — no restriction: every Rust `char` is UTF-8-encodable
    ///   (surrogates are structurally excluded from `char`), so this is a
    ///   no-op.
    ///
    /// The codec's codepoint range intersects with any bounds set via
    /// [`min_codepoint`](Self::min_codepoint) /
    /// [`max_codepoint`](Self::max_codepoint). Any other codec name is a
    /// usage error, reported when `.codec(...)` is called.
    pub fn codec(mut self, codec: &str) -> Self {
        check_codec(codec);
        self.handle = OnceLock::new();
        self.char_param_called = true;
        self.char_fields.codec = Some(codec.to_string());
        self
    }

    /// Set the minimum Unicode codepoint.
    pub fn min_codepoint(mut self, min_codepoint: u32) -> Self {
        self.handle = OnceLock::new();
        self.char_param_called = true;
        self.char_fields.min_codepoint = Some(min_codepoint);
        self
    }

    /// Set the maximum Unicode codepoint.
    pub fn max_codepoint(mut self, max_codepoint: u32) -> Self {
        self.handle = OnceLock::new();
        self.char_param_called = true;
        self.char_fields.max_codepoint = Some(max_codepoint);
        self
    }

    /// Include only characters from these Unicode general categories (e.g. `["L", "Nd"]`).
    ///
    /// Mutually exclusive with [`exclude_categories`](Self::exclude_categories).
    pub fn categories(mut self, categories: &[&str]) -> Self {
        self.handle = OnceLock::new();
        self.char_param_called = true;
        self.char_fields.categories = Some(categories.iter().map(|s| s.to_string()).collect());
        self
    }

    /// Exclude characters from these Unicode general categories.
    ///
    /// Mutually exclusive with [`categories`](Self::categories).
    pub fn exclude_categories(mut self, exclude_categories: &[&str]) -> Self {
        self.handle = OnceLock::new();
        self.char_param_called = true;
        self.char_fields.exclude_categories =
            Some(exclude_categories.iter().map(|s| s.to_string()).collect());
        self
    }

    /// Always include these specific characters, even if excluded by other filters.
    pub fn include_characters(mut self, include_characters: &str) -> Self {
        self.handle = OnceLock::new();
        self.char_param_called = true;
        self.char_fields.include_characters = Some(include_characters.to_string());
        self
    }

    /// Always exclude these specific characters.
    pub fn exclude_characters(mut self, exclude_characters: &str) -> Self {
        self.handle = OnceLock::new();
        self.char_param_called = true;
        self.char_fields.exclude_characters = Some(exclude_characters.to_string());
        self
    }

    fn handle(&self) -> &ffi::StringGenerator {
        self.handle.get_or_init(|| {
            if self.alphabet_called && self.char_param_called {
                invalid_argument!("Cannot combine .alphabet() with character methods.");
            }
            if let Some(max) = self.max_size {
                if self.min_size > max {
                    invalid_argument!("Cannot have max_size < min_size");
                }
            }
            let max_size = self
                .max_size
                .unwrap_or(if self.min_size > DEFAULT_MAX_SIZE {
                    self.min_size + DEFAULT_MAX_SIZE
                } else {
                    DEFAULT_MAX_SIZE
                });
            self.char_fields
                .build_text_handle(self.min_size as u64, max_size as u64)
        })
    }
}

impl Generator<String> for TextGenerator {
    fn do_draw(&self, tc: &TestCase) -> String {
        tc.generate_string(self.handle())
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
        handle: OnceLock::new(),
    }
}

/// Generator for single Unicode characters ([`char`]). Created by [`characters()`].
pub struct CharactersGenerator {
    char_fields: CharacterFields,
    handle: OnceLock<ffi::StringGenerator>,
}

impl CharactersGenerator {
    /// Restrict to characters encodable in the named codec.
    ///
    /// Supported values, and what each means for Rust `char`s:
    ///
    /// - `"ascii"` — codepoints `U+0000..=U+007F`; equivalent to
    ///   `.max_codepoint(0x7F)`.
    /// - `"latin-1"` (alias `"iso-8859-1"`) — codepoints `U+0000..=U+00FF`;
    ///   equivalent to `.max_codepoint(0xFF)`.
    /// - `"utf-8"` — no restriction: every Rust `char` is UTF-8-encodable
    ///   (surrogates are structurally excluded from `char`), so this is a
    ///   no-op.
    ///
    /// The codec's codepoint range intersects with any bounds set via
    /// [`min_codepoint`](Self::min_codepoint) /
    /// [`max_codepoint`](Self::max_codepoint). Any other codec name is a
    /// usage error, reported when `.codec(...)` is called.
    pub fn codec(mut self, codec: &str) -> Self {
        check_codec(codec);
        self.handle = OnceLock::new();
        self.char_fields.codec = Some(codec.to_string());
        self
    }

    /// Set the minimum Unicode codepoint.
    pub fn min_codepoint(mut self, min_codepoint: u32) -> Self {
        self.handle = OnceLock::new();
        self.char_fields.min_codepoint = Some(min_codepoint);
        self
    }

    /// Set the maximum Unicode codepoint.
    pub fn max_codepoint(mut self, max_codepoint: u32) -> Self {
        self.handle = OnceLock::new();
        self.char_fields.max_codepoint = Some(max_codepoint);
        self
    }

    /// Include only characters from these Unicode general categories (e.g. `["L", "Nd"]`).
    ///
    /// Mutually exclusive with [`exclude_categories`](Self::exclude_categories).
    pub fn categories(mut self, categories: &[&str]) -> Self {
        self.handle = OnceLock::new();
        self.char_fields.categories = Some(categories.iter().map(|s| s.to_string()).collect());
        self
    }

    /// Exclude characters from these Unicode general categories.
    ///
    /// Mutually exclusive with [`categories`](Self::categories).
    pub fn exclude_categories(mut self, exclude_categories: &[&str]) -> Self {
        self.handle = OnceLock::new();
        self.char_fields.exclude_categories =
            Some(exclude_categories.iter().map(|s| s.to_string()).collect());
        self
    }

    /// Always include these specific characters, even if excluded by other filters.
    pub fn include_characters(mut self, include_characters: &str) -> Self {
        self.handle = OnceLock::new();
        self.char_fields.include_characters = Some(include_characters.to_string());
        self
    }

    /// Always exclude these specific characters.
    pub fn exclude_characters(mut self, exclude_characters: &str) -> Self {
        self.handle = OnceLock::new();
        self.char_fields.exclude_characters = Some(exclude_characters.to_string());
        self
    }

    fn handle(&self) -> &ffi::StringGenerator {
        self.handle
            .get_or_init(|| self.char_fields.build_text_handle(1, 1))
    }

    /// Build a standalone text handle carrying this alphabet, for use as a
    /// regex alphabet constraint. Sized `0..=0` so an empty alphabet — legal
    /// for regex padding, which simply pads nothing — passes construction.
    pub(super) fn build_alphabet_handle(&self) -> ffi::StringGenerator {
        self.char_fields.build_text_handle(0, 0)
    }
}

impl Generator<char> for CharactersGenerator {
    fn do_draw(&self, tc: &TestCase) -> char {
        let s = tc.generate_string(self.handle());
        let mut chars = s.chars();
        let c = chars
            .next()
            .expect("expected a single character, got empty string");
        hegel_internal_assert!(
            chars.next().is_none(),
            "expected a single character, got multiple"
        );
        c
    }
}

/// Generate single Unicode characters ([`char`]).
///
/// See [`CharactersGenerator`] for builder methods.
pub fn characters() -> CharactersGenerator {
    CharactersGenerator {
        char_fields: CharacterFields::new(),
        handle: OnceLock::new(),
    }
}

/// Generator for strings matching a regex pattern. Created by [`from_regex()`].
///
/// By default the entire string matches the pattern. Use
/// [`fullmatch(false)`](Self::fullmatch) to generate strings that merely
/// contain a match.
pub struct RegexGenerator {
    pattern: String,
    fullmatch: bool,
    alphabet: Option<CharactersGenerator>,
    handle: OnceLock<ffi::StringGenerator>,
}

impl RegexGenerator {
    /// Set whether the entire string must match the pattern (the default), or
    /// merely contain a match.
    pub fn fullmatch(mut self, fullmatch: bool) -> Self {
        self.handle = OnceLock::new();
        self.fullmatch = fullmatch;
        self
    }

    /// Constrain which characters may appear in generated strings.
    pub fn alphabet(mut self, alphabet: CharactersGenerator) -> Self {
        self.handle = OnceLock::new();
        self.alphabet = Some(alphabet);
        self
    }

    fn handle(&self) -> &ffi::StringGenerator {
        self.handle.get_or_init(|| {
            let alphabet = self.alphabet.as_ref().map(|a| a.build_alphabet_handle());
            ffi::StringGenerator::regex(&self.pattern, self.fullmatch, alphabet.as_ref())
                .unwrap_or_else(|msg| invalid_argument!("{msg}"))
        })
    }
}

impl Generator<String> for RegexGenerator {
    fn do_draw(&self, tc: &TestCase) -> String {
        tc.generate_string(self.handle())
    }
}

/// Generate strings matching a regex pattern.
///
/// See [`RegexGenerator`] for builder methods.
pub fn from_regex(pattern: &str) -> RegexGenerator {
    RegexGenerator {
        pattern: pattern.to_string(),
        fullmatch: true,
        alphabet: None,
        handle: OnceLock::new(),
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
}

impl Generator<Vec<u8>> for BinaryGenerator {
    fn do_draw(&self, tc: &TestCase) -> Vec<u8> {
        if let Some(max) = self.max_size {
            if self.min_size > max {
                invalid_argument!("Cannot have max_size < min_size");
            }
        }
        let max_size = self
            .max_size
            .unwrap_or(if self.min_size > DEFAULT_MAX_SIZE {
                self.min_size + DEFAULT_MAX_SIZE
            } else {
                DEFAULT_MAX_SIZE
            });
        tc.generate_bytes(self.min_size, max_size)
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
pub struct EmailGenerator {
    handle: OnceLock<ffi::StringGenerator>,
}

impl Generator<String> for EmailGenerator {
    fn do_draw(&self, tc: &TestCase) -> String {
        let handle = self.handle.get_or_init(|| {
            ffi::StringGenerator::email().unwrap_or_else(|msg| invalid_argument!("{msg}"))
        });
        tc.generate_string(handle)
    }
}

/// Generate email address strings.
pub fn emails() -> EmailGenerator {
    EmailGenerator {
        handle: OnceLock::new(),
    }
}

/// Generator for URL strings. Created by [`urls()`].
pub struct UrlGenerator {
    handle: OnceLock<ffi::StringGenerator>,
}

impl Generator<String> for UrlGenerator {
    fn do_draw(&self, tc: &TestCase) -> String {
        let handle = self.handle.get_or_init(|| {
            ffi::StringGenerator::url().unwrap_or_else(|msg| invalid_argument!("{msg}"))
        });
        tc.generate_string(handle)
    }
}

/// Generate URL strings.
pub fn urls() -> UrlGenerator {
    UrlGenerator {
        handle: OnceLock::new(),
    }
}

/// Generator for domain name strings. Created by [`domains()`].
pub struct DomainGenerator {
    max_length: usize,
    handle: OnceLock<ffi::StringGenerator>,
}

impl DomainGenerator {
    /// Set the maximum length (must be between 4 and 255).
    pub fn max_length(mut self, max_length: usize) -> Self {
        self.handle = OnceLock::new();
        self.max_length = max_length;
        self
    }
}

impl Generator<String> for DomainGenerator {
    fn do_draw(&self, tc: &TestCase) -> String {
        let handle = self.handle.get_or_init(|| {
            if !(self.max_length >= 4 && self.max_length <= 255) {
                invalid_argument!("max_length must be between 4 and 255");
            }
            ffi::StringGenerator::domain(self.max_length as u64)
                .unwrap_or_else(|msg| invalid_argument!("{msg}"))
        });
        tc.generate_string(handle)
    }
}

/// Generate domain name strings.
///
/// See [`DomainGenerator`] for builder methods.
pub fn domains() -> DomainGenerator {
    DomainGenerator {
        max_length: 255,
        handle: OnceLock::new(),
    }
}

/// Generator for IP addresses. Created by [`ip_addresses()`].
///
/// Generates both IPv4 and IPv6 addresses.
pub struct IpAddressGenerator {}

impl IpAddressGenerator {
    /// Only generate IPv4 addresses.
    pub fn v4(self) -> Ipv4AddressGenerator {
        Ipv4AddressGenerator {}
    }

    /// Only generate IPv6 addresses.
    pub fn v6(self) -> Ipv6AddressGenerator {
        Ipv6AddressGenerator {}
    }
}

impl Generator<std::net::IpAddr> for IpAddressGenerator {
    fn do_draw(&self, tc: &TestCase) -> std::net::IpAddr {
        tc.start_span(labels::ONE_OF);
        let addr = if tc.generate_integer_i64(0, 1) == 0 {
            std::net::IpAddr::V4(tc.generate_ipv4())
        } else {
            std::net::IpAddr::V6(tc.generate_ipv6())
        };
        tc.stop_span(false);
        addr
    }
}

/// Generator for IPv4 addresses. Created by [`IpAddressGenerator::v4`].
pub struct Ipv4AddressGenerator {}

impl Generator<std::net::Ipv4Addr> for Ipv4AddressGenerator {
    fn do_draw(&self, tc: &TestCase) -> std::net::Ipv4Addr {
        tc.generate_ipv4()
    }
}

/// Generator for IPv6 addresses. Created by [`IpAddressGenerator::v6`].
pub struct Ipv6AddressGenerator {}

impl Generator<std::net::Ipv6Addr> for Ipv6AddressGenerator {
    fn do_draw(&self, tc: &TestCase) -> std::net::Ipv6Addr {
        tc.generate_ipv6()
    }
}

/// Generate IP addresses (IPv4 or IPv6).
///
/// See [`IpAddressGenerator`] for builder methods.
pub fn ip_addresses() -> IpAddressGenerator {
    IpAddressGenerator {}
}

/// Format a drawn date as `YYYY-MM-DD`, matching `st.dates().isoformat()`.
pub(crate) fn format_date(d: hegel_c::hegel_date_t) -> String {
    format!("{:04}-{:02}-{:02}", d.year, d.month, d.day)
}

/// Format a drawn time as `HH:MM:SS` or `HH:MM:SS.ffffff`, matching
/// `st.times().isoformat()`: the fractional part is present iff
/// `microsecond != 0`.
pub(crate) fn format_time(t: hegel_c::hegel_time_t) -> String {
    if t.microsecond == 0 {
        format!("{:02}:{:02}:{:02}", t.hour, t.minute, t.second)
    } else {
        format!(
            "{:02}:{:02}:{:02}.{:06}",
            t.hour, t.minute, t.second, t.microsecond
        )
    }
}

/// Generator for date strings in `YYYY-MM-DD` format. Created by
/// [`date_strings()`].
pub struct DateStringGenerator;

impl Generator<String> for DateStringGenerator {
    fn do_draw(&self, tc: &TestCase) -> String {
        format_date(tc.generate_date(full_ranges::MIN_DATE, full_ranges::MAX_DATE))
    }
}

/// Generate date `String`s in `YYYY-MM-DD` format (years 1–9999), matching
/// Python's `date.isoformat()`.
///
/// This generator is not configurable. For typed date values with
/// configurable bounds, see [`extras::chrono`](crate::extras::chrono)
/// (`naive_dates()`) or [`extras::jiff`](crate::extras::jiff) (`dates()`).
pub fn date_strings() -> DateStringGenerator {
    DateStringGenerator
}

/// Generator for time strings in `HH:MM:SS[.ffffff]` format. Created by
/// [`time_strings()`].
pub struct TimeStringGenerator;

impl Generator<String> for TimeStringGenerator {
    fn do_draw(&self, tc: &TestCase) -> String {
        format_time(tc.generate_time(full_ranges::MIDNIGHT, full_ranges::LAST_MICROSECOND))
    }
}

/// Generate time `String`s in `HH:MM:SS` format, matching Python's
/// `time.isoformat()`: a fractional `.ffffff` part (microseconds) is
/// appended iff it is non-zero.
///
/// This generator is not configurable. For typed time values with
/// configurable bounds, see [`extras::chrono`](crate::extras::chrono)
/// (`naive_times()`) or [`extras::jiff`](crate::extras::jiff) (`times()`).
pub fn time_strings() -> TimeStringGenerator {
    TimeStringGenerator
}

/// Generator for ISO 8601 datetime strings. Created by [`datetime_strings()`].
pub struct DateTimeStringGenerator;

impl Generator<String> for DateTimeStringGenerator {
    fn do_draw(&self, tc: &TestCase) -> String {
        let dt = tc.generate_datetime(full_ranges::MIN_DATETIME, full_ranges::MAX_DATETIME);
        format!("{}T{}", format_date(dt.date), format_time(dt.time))
    }
}

/// Generate ISO 8601 datetime `String`s (`YYYY-MM-DDTHH:MM:SS[.ffffff]`,
/// years 1–9999), matching Python's `datetime.isoformat()`.
///
/// This generator is not configurable. For typed datetime values with
/// configurable bounds, see [`extras::chrono`](crate::extras::chrono)
/// (`naive_datetimes()` / `datetimes()`) or
/// [`extras::jiff`](crate::extras::jiff) (`datetimes()`).
pub fn datetime_strings() -> DateTimeStringGenerator {
    DateTimeStringGenerator
}

/// Generator for UUID strings in canonical hyphenated form. Created by [`uuids()`].
///
/// By default generates UUIDs of any version. Use [`UuidsGenerator::version`]
/// to restrict to a specific RFC 4122 version (1–5).
pub struct UuidsGenerator {
    version: Option<u8>,
}

impl UuidsGenerator {
    /// Restrict to UUIDs of a specific version (1–5).
    pub fn version(mut self, version: u8) -> Self {
        self.version = Some(version);
        self
    }
}

impl Generator<String> for UuidsGenerator {
    fn do_draw(&self, tc: &TestCase) -> String {
        if let Some(v) = self.version {
            if !(1..=5).contains(&v) {
                invalid_argument!("UUID version must be between 1 and 5, got {v}");
            }
        }
        let b = tc.generate_uuid(self.version);
        format!(
            "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
            b[0],
            b[1],
            b[2],
            b[3],
            b[4],
            b[5],
            b[6],
            b[7],
            b[8],
            b[9],
            b[10],
            b[11],
            b[12],
            b[13],
            b[14],
            b[15],
        )
    }
}

/// Generate UUID `String`s in canonical hyphenated form, e.g.
/// `"a70f446c-05e3-42a9-a31b-f0d0545d6316"`.
///
/// See [`UuidsGenerator`] for builder methods.
pub fn uuids() -> UuidsGenerator {
    UuidsGenerator { version: None }
}

impl PrintableGenerator<String> for TextGenerator {
    fn do_draw_and_print(&self, tc: &TestCase, printer: &mut PrettyPrinter) -> String {
        draw_and_print_value(self, tc, printer)
    }
}

impl PrintableGenerator<char> for CharactersGenerator {
    fn do_draw_and_print(&self, tc: &TestCase, printer: &mut PrettyPrinter) -> char {
        draw_and_print_value(self, tc, printer)
    }
}

impl PrintableGenerator<String> for RegexGenerator {
    fn do_draw_and_print(&self, tc: &TestCase, printer: &mut PrettyPrinter) -> String {
        draw_and_print_value(self, tc, printer)
    }
}

impl PrintableGenerator<Vec<u8>> for BinaryGenerator {
    fn do_draw_and_print(&self, tc: &TestCase, printer: &mut PrettyPrinter) -> Vec<u8> {
        draw_and_print_value(self, tc, printer)
    }
}

impl PrintableGenerator<String> for EmailGenerator {
    fn do_draw_and_print(&self, tc: &TestCase, printer: &mut PrettyPrinter) -> String {
        draw_and_print_value(self, tc, printer)
    }
}

impl PrintableGenerator<String> for UrlGenerator {
    fn do_draw_and_print(&self, tc: &TestCase, printer: &mut PrettyPrinter) -> String {
        draw_and_print_value(self, tc, printer)
    }
}

impl PrintableGenerator<String> for DomainGenerator {
    fn do_draw_and_print(&self, tc: &TestCase, printer: &mut PrettyPrinter) -> String {
        draw_and_print_value(self, tc, printer)
    }
}

impl PrintableGenerator<std::net::IpAddr> for IpAddressGenerator {
    fn do_draw_and_print(&self, tc: &TestCase, printer: &mut PrettyPrinter) -> std::net::IpAddr {
        draw_and_print_value(self, tc, printer)
    }
}

impl PrintableGenerator<std::net::Ipv4Addr> for Ipv4AddressGenerator {
    fn do_draw_and_print(&self, tc: &TestCase, printer: &mut PrettyPrinter) -> std::net::Ipv4Addr {
        draw_and_print_value(self, tc, printer)
    }
}

impl PrintableGenerator<std::net::Ipv6Addr> for Ipv6AddressGenerator {
    fn do_draw_and_print(&self, tc: &TestCase, printer: &mut PrettyPrinter) -> std::net::Ipv6Addr {
        draw_and_print_value(self, tc, printer)
    }
}

impl PrintableGenerator<String> for DateStringGenerator {
    fn do_draw_and_print(&self, tc: &TestCase, printer: &mut PrettyPrinter) -> String {
        draw_and_print_value(self, tc, printer)
    }
}

impl PrintableGenerator<String> for TimeStringGenerator {
    fn do_draw_and_print(&self, tc: &TestCase, printer: &mut PrettyPrinter) -> String {
        draw_and_print_value(self, tc, printer)
    }
}

impl PrintableGenerator<String> for DateTimeStringGenerator {
    fn do_draw_and_print(&self, tc: &TestCase, printer: &mut PrettyPrinter) -> String {
        draw_and_print_value(self, tc, printer)
    }
}

impl PrintableGenerator<String> for UuidsGenerator {
    fn do_draw_and_print(&self, tc: &TestCase, printer: &mut PrettyPrinter) -> String {
        draw_and_print_value(self, tc, printer)
    }
}
