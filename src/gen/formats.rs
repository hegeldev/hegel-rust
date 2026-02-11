use super::{BasicGenerator, Generate};
use crate::cbor_helpers::{cbor_array, cbor_map};
use std::sync::OnceLock;

pub struct EmailGenerator {
    cached_basic: OnceLock<Option<BasicGenerator<String>>>,
}

impl Generate<String> for EmailGenerator {
    fn generate(&self) -> String {
        self.as_basic().unwrap().generate()
    }

    fn as_basic(&self) -> Option<BasicGenerator<String>> {
        self.cached_basic
            .get_or_init(|| Some(BasicGenerator::new(cbor_map! {"type" => "email"})))
            .clone()
    }
}

pub fn emails() -> EmailGenerator {
    EmailGenerator {
        cached_basic: OnceLock::new(),
    }
}

pub struct UrlGenerator {
    cached_basic: OnceLock<Option<BasicGenerator<String>>>,
}

impl Generate<String> for UrlGenerator {
    fn generate(&self) -> String {
        self.as_basic().unwrap().generate()
    }

    fn as_basic(&self) -> Option<BasicGenerator<String>> {
        self.cached_basic
            .get_or_init(|| Some(BasicGenerator::new(cbor_map! {"type" => "url"})))
            .clone()
    }
}

pub fn urls() -> UrlGenerator {
    UrlGenerator {
        cached_basic: OnceLock::new(),
    }
}

pub struct DomainGenerator {
    max_length: usize,
    cached_basic: OnceLock<Option<BasicGenerator<String>>>,
}

impl DomainGenerator {
    pub fn with_max_length(mut self, max: usize) -> Self {
        self.max_length = max;
        self.cached_basic = OnceLock::new();
        self
    }
}

impl Generate<String> for DomainGenerator {
    fn generate(&self) -> String {
        self.as_basic().unwrap().generate()
    }

    fn as_basic(&self) -> Option<BasicGenerator<String>> {
        self.cached_basic
            .get_or_init(|| {
                Some(BasicGenerator::new(cbor_map! {
                    "type" => "domain",
                    "max_length" => self.max_length as u64
                }))
            })
            .clone()
    }
}

pub fn domains() -> DomainGenerator {
    DomainGenerator {
        max_length: 255,
        cached_basic: OnceLock::new(),
    }
}

#[derive(Clone, Copy)]
pub enum IpVersion {
    V4,
    V6,
}

pub struct IpAddressGenerator {
    version: Option<IpVersion>,
    cached_basic: OnceLock<Option<BasicGenerator<String>>>,
}

impl IpAddressGenerator {
    pub fn v4(mut self) -> Self {
        self.version = Some(IpVersion::V4);
        self.cached_basic = OnceLock::new();
        self
    }

    pub fn v6(mut self) -> Self {
        self.version = Some(IpVersion::V6);
        self.cached_basic = OnceLock::new();
        self
    }
}

impl Generate<String> for IpAddressGenerator {
    fn generate(&self) -> String {
        self.as_basic().unwrap().generate()
    }

    fn as_basic(&self) -> Option<BasicGenerator<String>> {
        self.cached_basic
            .get_or_init(|| match self.version {
                Some(IpVersion::V4) => Some(BasicGenerator::new(cbor_map! {"type" => "ipv4"})),
                Some(IpVersion::V6) => Some(BasicGenerator::new(cbor_map! {"type" => "ipv6"})),
                None => Some(BasicGenerator::new(cbor_map! {
                    "one_of" => cbor_array![
                        cbor_map!{"type" => "ipv4"},
                        cbor_map!{"type" => "ipv6"}
                    ]
                })),
            })
            .clone()
    }
}

pub fn ip_addresses() -> IpAddressGenerator {
    IpAddressGenerator {
        version: None,
        cached_basic: OnceLock::new(),
    }
}

pub struct DateGenerator {
    cached_basic: OnceLock<Option<BasicGenerator<String>>>,
}

impl Generate<String> for DateGenerator {
    fn generate(&self) -> String {
        self.as_basic().unwrap().generate()
    }

    fn as_basic(&self) -> Option<BasicGenerator<String>> {
        self.cached_basic
            .get_or_init(|| Some(BasicGenerator::new(cbor_map! {"type" => "date"})))
            .clone()
    }
}

pub fn dates() -> DateGenerator {
    DateGenerator {
        cached_basic: OnceLock::new(),
    }
}

pub struct TimeGenerator {
    cached_basic: OnceLock<Option<BasicGenerator<String>>>,
}

impl Generate<String> for TimeGenerator {
    fn generate(&self) -> String {
        self.as_basic().unwrap().generate()
    }

    fn as_basic(&self) -> Option<BasicGenerator<String>> {
        self.cached_basic
            .get_or_init(|| Some(BasicGenerator::new(cbor_map! {"type" => "time"})))
            .clone()
    }
}

pub fn times() -> TimeGenerator {
    TimeGenerator {
        cached_basic: OnceLock::new(),
    }
}

pub struct DateTimeGenerator {
    cached_basic: OnceLock<Option<BasicGenerator<String>>>,
}

impl Generate<String> for DateTimeGenerator {
    fn generate(&self) -> String {
        self.as_basic().unwrap().generate()
    }

    fn as_basic(&self) -> Option<BasicGenerator<String>> {
        self.cached_basic
            .get_or_init(|| Some(BasicGenerator::new(cbor_map! {"type" => "datetime"})))
            .clone()
    }
}

pub fn datetimes() -> DateTimeGenerator {
    DateTimeGenerator {
        cached_basic: OnceLock::new(),
    }
}
