use hegel::TestCase;
use hegel::generators;

#[hegel::test]
fn test_from_regex_generates_matching_string(tc: TestCase) {
    let value = tc.draw(generators::from_regex("[a-z]{3,5}"));
    assert!(value.len() >= 3);
}

#[hegel::test]
fn test_from_regex_fullmatch(tc: TestCase) {
    let value = tc.draw(generators::from_regex("[0-9]{4}").fullmatch(true));
    assert_eq!(value.len(), 4);
    assert!(value.chars().all(|c| c.is_ascii_digit()));
}

#[hegel::test]
fn test_emails_generates_valid_email(tc: TestCase) {
    let value = tc.draw(generators::emails());
    assert!(value.contains('@'), "email should contain @: {}", value);
}

#[hegel::test]
fn test_urls_generates_valid_url(tc: TestCase) {
    let value = tc.draw(generators::urls());
    assert!(
        value.starts_with("http://") || value.starts_with("https://"),
        "url should start with http(s)://: {}",
        value
    );
}

#[hegel::test]
fn test_domains_generates_valid_domain(tc: TestCase) {
    let value = tc.draw(generators::domains());
    assert!(
        value.contains('.'),
        "domain should contain a dot: {}",
        value
    );
    assert!(value.len() <= 255);
}

#[hegel::test]
fn test_domains_with_max_length(tc: TestCase) {
    let value = tc.draw(generators::domains().max_length(50));
    assert!(value.len() <= 50);
}

#[hegel::test]
fn test_ip_addresses_v4(tc: TestCase) {
    let value = tc.draw(generators::ip_addresses().v4());
    let parts: Vec<&str> = value.split('.').collect();
    assert_eq!(parts.len(), 4, "IPv4 should have 4 octets: {}", value);
}

#[hegel::test]
fn test_ip_addresses_v6(tc: TestCase) {
    let value = tc.draw(generators::ip_addresses().v6());
    assert!(value.contains(':'), "IPv6 should contain colons: {}", value);
}

#[hegel::test]
fn test_ip_addresses_either(tc: TestCase) {
    let value = tc.draw(generators::ip_addresses());
    assert!(
        value.contains('.') || value.contains(':'),
        "IP should be v4 or v6: {}",
        value
    );
}

#[hegel::test]
fn test_dates_generates_valid_date(tc: TestCase) {
    let value = tc.draw(generators::dates());
    // YYYY-MM-DD format
    let parts: Vec<&str> = value.split('-').collect();
    assert_eq!(parts.len(), 3, "date should have 3 parts: {}", value);
    assert_eq!(parts[0].len(), 4, "year should be 4 digits: {}", value);
}

#[hegel::test]
fn test_times_generates_valid_time(tc: TestCase) {
    let value = tc.draw(generators::times());
    // HH:MM:SS format
    let parts: Vec<&str> = value.split(':').collect();
    assert!(
        parts.len() >= 2,
        "time should have at least 2 parts: {}",
        value
    );
}

#[hegel::test]
fn test_datetimes_generates_valid_datetime(tc: TestCase) {
    let value = tc.draw(generators::datetimes());
    // ISO 8601 format contains T separator or space
    assert!(
        value.contains('T') || value.contains(' '),
        "datetime should contain T or space: {}",
        value
    );
}

// Tests that exercise the as_basic() path by wrapping generators in vecs/maps
// This covers the parse closures inside as_basic() for each string generator type

#[hegel::test]
fn test_regex_in_vec_uses_basic_path(tc: TestCase) {
    let values: Vec<String> = tc.draw(
        generators::vecs(generators::from_regex("[a-z]+").fullmatch(true))
            .min_size(1)
            .max_size(3),
    );
    assert!(!values.is_empty());
}

#[hegel::test]
fn test_emails_in_vec_uses_basic_path(tc: TestCase) {
    let values: Vec<String> = tc.draw(
        generators::vecs(generators::emails())
            .min_size(1)
            .max_size(3),
    );
    assert!(values.iter().all(|v| v.contains('@')));
}

#[hegel::test]
fn test_urls_in_vec_uses_basic_path(tc: TestCase) {
    let values: Vec<String> = tc.draw(generators::vecs(generators::urls()).min_size(1).max_size(3));
    assert!(
        values
            .iter()
            .all(|v| v.starts_with("http://") || v.starts_with("https://"))
    );
}

#[hegel::test]
fn test_domains_in_vec_uses_basic_path(tc: TestCase) {
    let values: Vec<String> = tc.draw(
        generators::vecs(generators::domains().max_length(100))
            .min_size(1)
            .max_size(3),
    );
    assert!(values.iter().all(|v| v.contains('.')));
}

#[hegel::test]
fn test_ip_addresses_in_vec_uses_basic_path(tc: TestCase) {
    let values: Vec<String> = tc.draw(
        generators::vecs(generators::ip_addresses())
            .min_size(1)
            .max_size(3),
    );
    assert!(!values.is_empty());
}

#[hegel::test]
fn test_dates_in_vec_uses_basic_path(tc: TestCase) {
    let values: Vec<String> = tc.draw(
        generators::vecs(generators::dates())
            .min_size(1)
            .max_size(3),
    );
    assert!(values.iter().all(|v| v.split('-').count() == 3));
}

#[hegel::test]
fn test_times_in_vec_uses_basic_path(tc: TestCase) {
    let values: Vec<String> = tc.draw(
        generators::vecs(generators::times())
            .min_size(1)
            .max_size(3),
    );
    assert!(values.iter().all(|v| v.contains(':')));
}

#[hegel::test]
fn test_datetimes_in_vec_uses_basic_path(tc: TestCase) {
    let values: Vec<String> = tc.draw(
        generators::vecs(generators::datetimes())
            .min_size(1)
            .max_size(3),
    );
    assert!(values.iter().all(|v| v.contains('T') || v.contains(' ')));
}
