mod common;

use common::utils::{assert_all_examples, find_any};
use hegel::generators as gs;

#[test]
fn test_characters_single_char() {
    assert_all_examples(gs::characters(), |c: &char| c.len_utf8() > 0);
}

#[test]
fn test_characters_ascii() {
    assert_all_examples(gs::characters().codec("ascii"), |c: &char| c.is_ascii());
}

#[hegel::test]
fn test_characters_codepoint_range(tc: hegel::TestCase) {
    let lo = tc.draw(gs::integers::<u32>().min_value(0).max_value(0x10FFFF));
    let hi = tc.draw(gs::integers::<u32>().min_value(lo).max_value(0x10FFFF));
    // Skip ranges that fall entirely within the surrogate block (0xD800-0xDFFF);
    // those have no valid Unicode scalar values.
    tc.assume(!(lo >= 0xD800 && hi <= 0xDFFF));
    let c: char = tc.draw(gs::characters().min_codepoint(lo).max_codepoint(hi));
    let cp = c as u32;
    assert!(cp >= lo && cp <= hi);
}

#[test]
fn test_characters_lu() {
    assert_all_examples(gs::characters().categories(&["Lu"]), |c: &char| {
        c.is_uppercase()
    });
}

#[test]
fn test_characters_exclude_categories() {
    assert_all_examples(gs::characters().exclude_categories(&["Lu"]), |c: &char| {
        !c.is_uppercase()
    });
}

#[test]
fn test_characters_include_characters() {
    assert_all_examples(
        gs::characters().categories(&[]).include_characters("xyz"),
        |c: &char| "xyz".contains(*c),
    );
}

#[hegel::test]
fn test_characters_exclude_characters(tc: hegel::TestCase) {
    let exclude = tc.draw(gs::text().codec("ascii"));
    let c: char = tc.draw(gs::characters().codec("ascii").exclude_characters(&exclude));
    assert!(!exclude.contains(c));
}

#[hegel::test]
fn test_text_alphabet(tc: hegel::TestCase) {
    let alphabet = tc.draw(gs::text().codec("ascii").min_size(1));
    let s = tc.draw(gs::text().alphabet(&alphabet));
    assert!(s.chars().all(|c| alphabet.contains(c)));
}

#[test]
fn test_find_all_alphabet() {
    find_any(gs::text().alphabet("abc").min_size(10), |s: &String| {
        s.contains('a') && s.contains('b') && s.contains('c')
    });
}

#[test]
fn test_text_single_char_alphabet() {
    assert_all_examples(
        gs::text().alphabet("x").min_size(1).max_size(5),
        |s: &String| !s.is_empty() && s.chars().all(|c| c == 'x'),
    );
}

#[test]
fn test_text_codec_ascii() {
    assert_all_examples(gs::text().codec("ascii"), |s: &String| s.is_ascii());
}

#[hegel::test]
fn test_text_codepoint_range(tc: hegel::TestCase) {
    let lo = tc.draw(gs::integers::<u32>().min_value(0).max_value(0x10FFFF));
    let hi = tc.draw(gs::integers::<u32>().min_value(lo).max_value(0x10FFFF));
    // Skip ranges that fall entirely within the surrogate block (0xD800-0xDFFF);
    // those have no valid Unicode scalar values.
    tc.assume(!(lo >= 0xD800 && hi <= 0xDFFF));
    let s: String = tc.draw(gs::text().min_codepoint(lo).max_codepoint(hi));
    assert!(s.chars().all(|c| {
        let cp = c as u32;
        cp >= lo && cp <= hi
    }));
}

#[test]
fn test_text_categories() {
    assert_all_examples(gs::text().categories(&["Lu"]).max_size(20), |s: &String| {
        s.chars().all(|c| c.is_uppercase())
    });
}

#[test]
fn test_text_exclude_categories() {
    assert_all_examples(
        gs::text().exclude_categories(&["Lu"]).max_size(20),
        |s: &String| s.chars().all(|c| !c.is_uppercase()),
    );
}

#[test]
fn test_text_include_characters() {
    assert_all_examples(
        gs::text()
            .categories(&[])
            .include_characters("xyz")
            .max_size(20),
        |s: &String| s.chars().all(|c| "xyz".contains(c)),
    );
}

#[hegel::test]
fn test_text_exclude_characters(tc: hegel::TestCase) {
    let exclude = tc.draw(gs::text().codec("ascii"));
    let s = tc.draw(gs::text().codec("ascii").exclude_characters(&exclude));
    assert!(!s.chars().any(|c| exclude.contains(c)));
}

#[test]
fn test_regex_with_alphabet() {
    assert_all_examples(
        gs::from_regex("[a-z]+")
            .fullmatch(true)
            .alphabet(gs::characters().max_codepoint(0x7F)),
        |s: &String| !s.is_empty() && s.chars().all(|c| c.is_ascii_lowercase()),
    );
}

// --- Special schema generators ---

#[test]
fn test_dates_format() {
    assert_all_examples(gs::dates(), |s: &String| {
        // Must match YYYY-MM-DD. Accept any year (server generates pre-1970 dates).
        let parts: Vec<&str> = s.split('-').collect();
        if parts.len() != 3 || parts[0].len() != 4 {
            return false;
        }
        let month: u32 = parts[1].parse().unwrap_or(0);
        let day: u32 = parts[2].parse().unwrap_or(0);
        parts[0].chars().all(|c| c.is_ascii_digit())
            && (1..=12).contains(&month)
            && (1..=31).contains(&day)
    });
}

#[test]
fn test_times_format() {
    assert_all_examples(gs::times(), |s: &String| {
        // HH:MM:SS with optional fractional seconds (server may produce microseconds).
        let parts: Vec<&str> = s.splitn(3, ':').collect();
        if parts.len() != 3 {
            return false;
        }
        let hour: u32 = parts[0].parse().unwrap_or(99);
        let min: u32 = parts[1].parse().unwrap_or(99);
        // Second field may have a fractional part: "SS" or "SS.ffffff"
        let sec: u32 = parts[2]
            .splitn(2, '.')
            .next()
            .unwrap_or("99")
            .parse()
            .unwrap_or(99);
        hour <= 23 && min <= 59 && sec <= 59
    });
}

#[test]
fn test_datetimes_format() {
    assert_all_examples(gs::datetimes(), |s: &String| {
        // YYYY-MM-DDTHH:MM:SS with optional fractional seconds. Accept any year.
        let parts: Vec<&str> = s.splitn(2, 'T').collect();
        if parts.len() != 2 {
            return false;
        }
        let date_parts: Vec<&str> = parts[0].split('-').collect();
        if date_parts.len() != 3 || date_parts[0].len() != 4 {
            return false;
        }
        let month: u32 = date_parts[1].parse().unwrap_or(0);
        let day: u32 = date_parts[2].parse().unwrap_or(0);
        if !(date_parts[0].chars().all(|c| c.is_ascii_digit())
            && (1..=12).contains(&month)
            && (1..=31).contains(&day))
        {
            return false;
        }
        let time_parts: Vec<&str> = parts[1].splitn(3, ':').collect();
        if time_parts.len() != 3 {
            return false;
        }
        let hour: u32 = time_parts[0].parse().unwrap_or(99);
        let min: u32 = time_parts[1].parse().unwrap_or(99);
        let sec: u32 = time_parts[2]
            .splitn(2, '.')
            .next()
            .unwrap_or("99")
            .parse()
            .unwrap_or(99);
        hour <= 23 && min <= 59 && sec <= 59
    });
}

#[test]
fn test_ip_addresses_format() {
    assert_all_examples(gs::ip_addresses(), |s: &String| {
        // Accept any valid IPv4 or IPv6 address string (including compressed IPv6).
        s.parse::<std::net::IpAddr>().is_ok()
    });
}

#[test]
fn test_ip_addresses_v4_only() {
    assert_all_examples(gs::ip_addresses().v4(), |s: &String| {
        let parts: Vec<&str> = s.split('.').collect();
        parts.len() == 4 && parts.iter().all(|p| p.parse::<u32>().is_ok_and(|n| n <= 255))
    });
}

#[test]
fn test_ip_addresses_v6_only() {
    assert_all_examples(gs::ip_addresses().v6(), |s: &String| {
        // Accept any valid IPv6 address string (including compressed form like "::").
        s.parse::<std::net::Ipv6Addr>().is_ok()
    });
}

#[test]
fn test_domains_format() {
    assert_all_examples(gs::domains(), |s: &String| {
        // At least two dot-separated labels, each non-empty with valid hostname chars.
        // Server generates mixed case (e.g. "A.COM"), so accept uppercase too.
        let parts: Vec<&str> = s.split('.').collect();
        parts.len() >= 2
            && parts.iter().all(|p| {
                !p.is_empty()
                    && p.chars()
                        .all(|c| c.is_ascii_alphanumeric() || c == '-')
            })
            && s.len() <= 255
    });
}

#[test]
fn test_emails_format() {
    assert_all_examples(gs::emails(), |s: &String| {
        // Must contain exactly one '@' with non-empty user and domain containing a dot.
        // Server generates mixed case and digits, so only check structure.
        let parts: Vec<&str> = s.splitn(2, '@').collect();
        if parts.len() != 2 {
            return false;
        }
        let user = parts[0];
        let domain = parts[1];
        !user.is_empty() && !domain.is_empty() && domain.contains('.')
    });
}

#[test]
fn test_urls_format() {
    assert_all_examples(gs::urls(), |s: &String| {
        (s.starts_with("http://") || s.starts_with("https://"))
            && s.len() > 7
    });
}
