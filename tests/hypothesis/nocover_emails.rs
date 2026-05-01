//! Ported from hypothesis-python/tests/nocover/test_emails.py
//!
//! Individually-skipped tests:
//! - `test_can_restrict_email_domains`: `emails(domains=just("mydomain.com"))` has no
//!   counterpart — hegel-rust's `EmailGenerator` exposes no `domains` parameter.

use crate::common::utils::assert_all_examples;
use hegel::generators as gs;

#[test]
fn test_is_valid_email() {
    assert_all_examples(gs::emails(), |address: &String| {
        let at_pos = match address.rfind('@') {
            Some(p) => p,
            None => return false,
        };
        let local = &address[..at_pos];
        let domain = &address[at_pos + 1..];
        address.len() <= 254
            && !local.is_empty()
            && !domain.is_empty()
            && !domain.to_lowercase().ends_with(".arpa")
    });
}
