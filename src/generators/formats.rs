use super::{BasicGenerator, Generate, TestCaseData};
use crate::cbor_helpers::{cbor_array, cbor_map};
use ciborium::Value;

pub struct EmailGenerator;

impl Generate<String> for EmailGenerator {
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

impl Generate<String> for UrlGenerator {
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
    pub fn with_max_length(mut self, max: usize) -> Self {
        self.max_length = max;
        self
    }

    fn build_schema(&self) -> Value {
        cbor_map! {
            "type" => "domain",
            "max_length" => self.max_length as u64
        }
    }
}

impl Generate<String> for DomainGenerator {
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

impl Generate<String> for IpAddressGenerator {
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

impl Generate<String> for DateGenerator {
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

impl Generate<String> for TimeGenerator {
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

impl Generate<String> for DateTimeGenerator {
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
