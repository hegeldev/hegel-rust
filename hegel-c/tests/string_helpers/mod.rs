use hegel_c::hegel_string_t;
use std::ffi::{CStr, CString};

pub trait TestStringInput {
    fn string_bytes(&self) -> &[u8];
}

impl TestStringInput for str {
    fn string_bytes(&self) -> &[u8] {
        self.as_bytes()
    }
}

impl TestStringInput for [u8] {
    fn string_bytes(&self) -> &[u8] {
        self
    }
}

impl<const N: usize> TestStringInput for [u8; N] {
    fn string_bytes(&self) -> &[u8] {
        self
    }
}

impl TestStringInput for CStr {
    fn string_bytes(&self) -> &[u8] {
        self.to_bytes()
    }
}

impl TestStringInput for CString {
    fn string_bytes(&self) -> &[u8] {
        self.as_bytes()
    }
}

impl<T: TestStringInput + ?Sized> TestStringInput for &T {
    fn string_bytes(&self) -> &[u8] {
        (*self).string_bytes()
    }
}

pub fn test_string(value: &(impl TestStringInput + ?Sized)) -> hegel_string_t {
    let bytes = value.string_bytes();
    hegel_string_t {
        data: bytes.as_ptr().cast(),
        len: bytes.len(),
    }
}
