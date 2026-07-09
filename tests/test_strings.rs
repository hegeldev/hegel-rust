mod common;

use common::utils::{assert_all_examples, assert_no_examples, find_any};
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
    tc.assume(lo < 0xD800 || hi > 0xDFFF);
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
fn test_characters_exclude_categories_with_bounded_range_compiles() {
    assert_all_examples(
        gs::characters()
            .exclude_categories(&["Lu"])
            .min_codepoint(0x30)
            .max_codepoint(0x39),
        |c: &char| c.is_ascii_digit(),
    );
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
    tc.assume(lo < 0xD800 || hi > 0xDFFF);
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

#[test]
fn test_dates_format() {
    assert_all_examples(gs::dates(), |s: &String| {
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
        let parts: Vec<&str> = s.splitn(3, ':').collect();
        if parts.len() != 3 {
            return false;
        }
        let hour: u32 = parts[0].parse().unwrap_or(99);
        let min: u32 = parts[1].parse().unwrap_or(99);
        let sec: u32 = parts[2]
            .split('.')
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
            .split('.')
            .next()
            .unwrap_or("99")
            .parse()
            .unwrap_or(99);
        hour <= 23 && min <= 59 && sec <= 59
    });
}

const NIL_UUID: &str = "00000000-0000-0000-0000-000000000000";

#[test]
fn test_no_nil_uuid_by_default() {
    assert_no_examples(gs::uuids(), |s: &String| s == NIL_UUID);
}

#[test]
fn test_uuids_have_canonical_form() {
    assert_all_examples(gs::uuids(), |s: &String| {
        let bytes = s.as_bytes();
        bytes.len() == 36
            && bytes[8] == b'-'
            && bytes[13] == b'-'
            && bytes[18] == b'-'
            && bytes[23] == b'-'
            && bytes
                .iter()
                .enumerate()
                .all(|(i, b)| matches!(i, 8 | 13 | 18 | 23) || b.is_ascii_hexdigit())
    });
}

#[test]
fn test_domains_format() {
    assert_all_examples(gs::domains(), |s: &String| {
        let parts: Vec<&str> = s.split('.').collect();
        parts.len() >= 2
            && parts
                .iter()
                .all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'))
            && s.len() <= 255
    });
}

#[test]
fn test_can_generate_specified_version() {
    for version in 1u8..=5 {
        assert_all_examples(gs::uuids().version(version), move |s: &String| {
            let bytes = s.as_bytes();
            let version_digit = (bytes[14] as char).to_digit(16);
            let variant_digit = (bytes[19] as char).to_digit(16);
            version_digit == Some(u32::from(version)) && matches!(variant_digit, Some(0x8..=0xb))
        });
    }
}

#[test]
fn test_emails_format() {
    assert_all_examples(gs::emails(), |s: &String| {
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
        (s.starts_with("http://") || s.starts_with("https://")) && s.len() > 7
    });
}

mod pbtkit_bytes {
    use crate::common::utils::{Minimal, assert_all_examples, minimal};
    use hegel::generators as gs;

    #[test]
    fn test_finds_short_binary() {
        let b = minimal(gs::binary().max_size(10), |b: &Vec<u8>| !b.is_empty());
        assert_eq!(b, vec![0u8]);
    }

    #[test]
    fn test_shrinks_bytes_to_minimal() {
        let b = Minimal::new(gs::binary().min_size(1).max_size(5), |b: &Vec<u8>| {
            b.contains(&0xFFu8)
        })
        .test_cases(1000)
        .run();
        assert_eq!(b, vec![0xFFu8]);
    }

    #[test]
    fn test_binary_respects_size_bounds() {
        assert_all_examples(gs::binary().min_size(2).max_size(4), |b: &Vec<u8>| {
            (2..=4).contains(&b.len())
        });
    }

    #[test]
    fn test_shrinks_bytes_with_constraints() {
        let b = Minimal::new(gs::binary().min_size(2).max_size(10), |b: &Vec<u8>| {
            b.iter().map(|&x| x as u32).sum::<u32>() > 10
        })
        .test_cases(1000)
        .run();
        assert_eq!(b.len(), 2);
        assert_eq!(b.iter().map(|&x| x as u32).sum::<u32>(), 11);
    }

    #[test]
    fn test_shrinks_bytes_to_simplest() {
        let b = minimal(gs::binary().max_size(10), |b: &Vec<u8>| {
            b.iter().map(|&x| x as u32).sum::<u32>() == 0
        });
        assert_eq!(b, Vec::<u8>::new());
    }
}

mod pbtkit_text {
    use crate::common::utils::{assert_all_examples, expect_panic, minimal};
    use hegel::generators as gs;
    use hegel::{Hegel, Settings};

    #[test]
    fn test_text_basic() {
        assert_all_examples(gs::text().min_size(1).max_size(5), |s: &String| {
            let len = s.chars().count();
            (1..=5).contains(&len)
        });
    }

    #[test]
    fn test_text_ascii() {
        assert_all_examples(
            gs::text().min_codepoint(32).max_codepoint(126),
            |s: &String| s.chars().all(|c| (32..=126).contains(&(c as u32))),
        );
    }

    #[test]
    fn test_text_shrinks_to_short() {
        let s = minimal(
            gs::text()
                .min_codepoint(b'a' as u32)
                .max_codepoint(b'z' as u32),
            |s: &String| !s.is_empty(),
        );
        assert_eq!(s, "a");
    }

    #[test]
    fn test_text_shrinks_characters() {
        let s = minimal(
            gs::text()
                .min_codepoint(b'a' as u32)
                .max_codepoint(b'z' as u32)
                .min_size(1)
                .max_size(5),
            |s: &String| s.contains('z'),
        );
        assert_eq!(s, "z");
    }

    #[test]
    fn test_text_no_surrogates() {
        assert_all_examples(
            gs::text().min_codepoint(0xD700).max_codepoint(0xE000),
            |s: &String| s.chars().all(|c| !(0xD800..=0xDFFF).contains(&(c as u32))),
        );
    }

    #[test]
    fn test_text_unicode_shrinks() {
        let s = minimal(
            gs::text()
                .min_codepoint(128)
                .max_codepoint(256)
                .min_size(1)
                .max_size(3),
            |s: &String| s.chars().any(|c| (c as u32) >= 200),
        );
        assert_eq!(s.chars().count(), 1);
        assert!(s.chars().all(|c| (c as u32) >= 200));
    }

    #[test]
    fn test_text_shrinks_to_simplest() {
        let s = minimal(
            gs::text()
                .min_codepoint(b'a' as u32)
                .max_codepoint(b'z' as u32)
                .max_size(5),
            |_: &String| true,
        );
        assert_eq!(s, "");
    }

    #[test]
    fn test_text_sorts_characters() {
        let s = minimal(
            gs::text()
                .min_codepoint(b'a' as u32)
                .max_codepoint(b'z' as u32)
                .min_size(3)
                .max_size(5),
            |s: &String| {
                let chars: Vec<char> = s.chars().collect();
                chars.len() >= 3 && chars.windows(2).all(|w| w[0] > w[1])
            },
        );
        let chars: Vec<char> = s.chars().collect();
        assert!(chars.len() >= 3);
        assert!(chars.windows(2).all(|w| w[0] > w[1]));
    }

    #[test]
    fn test_text_redistributes_to_empty() {
        let (s1, s2) = minimal(
            gs::tuples!(
                gs::text()
                    .min_codepoint(b'a' as u32)
                    .max_codepoint(b'z' as u32)
                    .max_size(10),
                gs::text()
                    .min_codepoint(b'a' as u32)
                    .max_codepoint(b'z' as u32)
                    .max_size(10),
            ),
            |(s1, s2): &(String, String)| s1.chars().count() + s2.chars().count() >= 3,
        );
        assert!(s1.is_empty() || s2.is_empty());
    }

    #[test]
    fn test_text_redistributes_pair() {
        let (s1, s2) = minimal(
            gs::tuples!(
                gs::text()
                    .min_codepoint(b'a' as u32)
                    .max_codepoint(b'z' as u32)
                    .min_size(1)
                    .max_size(10),
                gs::text()
                    .min_codepoint(b'a' as u32)
                    .max_codepoint(b'z' as u32)
                    .min_size(1)
                    .max_size(10),
            ),
            |(s1, s2): &(String, String)| s1.chars().count() + s2.chars().count() >= 5,
        );
        assert!(!s1.is_empty());
        assert!(!s2.is_empty());
    }

    #[test]
    fn test_draw_string_invalid_range() {
        expect_panic(
            || {
                Hegel::new(|tc| {
                    let _: String = tc.draw(gs::text().min_codepoint(200).max_codepoint(100));
                })
                .settings(Settings::new().test_cases(1).database(None))
                .run();
            },
            "InvalidArgument",
        );
    }
}

mod simple_strings {
    use crate::common::utils::{assert_all_examples, minimal};
    use hegel::generators::{self as gs};

    #[test]
    fn test_can_minimize_up_to_zero() {
        let s = minimal(gs::text(), |s: &String| s.chars().any(|c| c <= '0'));
        assert_eq!(s, "0");
    }

    #[test]
    fn test_minimizes_towards_ascii_zero() {
        let s = minimal(gs::text(), |s: &String| s.chars().any(|c| c < '0'));
        assert_eq!(s, "/");
    }

    #[test]
    fn test_can_handle_large_codepoints() {
        let s = minimal(gs::text(), |s: &String| s.as_str() >= "\u{2603}");
        assert_eq!(s, "\u{2603}");
    }

    #[test]
    fn test_can_find_mixed_ascii_and_non_ascii_strings() {
        let s = minimal(gs::text(), |s: &String| {
            s.chars().any(|c| c >= '\u{2603}') && s.chars().any(|c| c as u32 <= 127)
        });
        assert_eq!(s.chars().count(), 2);
        let mut chars: Vec<char> = s.chars().collect();
        chars.sort();
        assert_eq!(chars, vec!['0', '\u{2603}']);
    }

    #[test]
    fn test_will_find_ascii_examples_given_the_chance() {
        let s = minimal(
            gs::tuples!(gs::text().max_size(1), gs::text().max_size(1)),
            |s: &(String, String)| !s.0.is_empty() && s.0 < s.1,
        );
        let c0 = s.0.chars().next().unwrap();
        let c1 = s.1.chars().next().unwrap();
        assert_eq!(c1 as u32, c0 as u32 + 1);
        assert!(s.0 == "0" || s.1 == "0");
    }

    #[test]
    fn test_minimisation_consistent_with_characters() {
        let s = minimal(gs::text().alphabet("FEDCBA").min_size(3), |_: &String| true);
        assert_eq!(s, "AAA");
    }

    #[test]
    fn test_finds_single_element_strings() {
        let s = minimal(gs::text(), |s: &String| !s.is_empty());
        assert_eq!(s, "0");
    }

    #[test]
    fn test_binary_respects_max_size() {
        assert_all_examples(gs::binary().max_size(5), |x: &Vec<u8>| x.len() <= 5);
    }

    #[test]
    fn test_does_not_simplify_into_surrogates() {
        let f = minimal(gs::text(), |s: &String| s.as_str() >= "\u{e000}");
        assert_eq!(f, "\u{e000}");

        let size: usize = 2;
        let f = minimal(gs::text().min_size(size), move |s: &String| {
            s.chars().filter(|&c| c >= '\u{e000}').count() >= size
        });
        assert_eq!(f, "\u{e000}".repeat(size));
    }

    #[test]
    fn test_respects_alphabet_if_list() {
        assert_all_examples(gs::text().alphabet("ab"), |s: &String| {
            s.chars().all(|c| c == 'a' || c == 'b')
        });
    }

    #[test]
    fn test_respects_alphabet_if_string() {
        assert_all_examples(gs::text().alphabet("cdef"), |s: &String| {
            s.chars().all(|c| "cdef".contains(c))
        });
    }

    #[test]
    fn test_can_encode_as_utf8() {
        assert_all_examples(gs::text(), |s: &String| {
            std::str::from_utf8(s.as_bytes()).is_ok()
        });
    }

    #[test]
    fn test_can_blacklist_newlines() {
        assert_all_examples(gs::text().exclude_characters("\n"), |s: &String| {
            !s.contains('\n')
        });
    }

    #[test]
    fn test_can_exclude_newlines_by_category() {
        assert_all_examples(
            gs::text().exclude_categories(&["Cc", "Cs"]),
            |s: &String| !s.contains('\n'),
        );
    }

    #[test]
    fn test_can_restrict_to_ascii_only() {
        assert_all_examples(gs::text().max_codepoint(127), |s: &String| s.is_ascii());
    }

    #[test]
    fn test_can_set_max_size_large() {
        assert_all_examples(gs::text().max_size(1_000_000), |_: &String| true);
    }
}

mod simple_characters {
    use crate::common::utils::{assert_no_examples, expect_panic, find_any, minimal};
    use hegel::generators::{self as gs, Generator};
    use hegel::{Hegel, Settings};

    fn expect_generator_panic<T, G>(generator: G, pattern: &str)
    where
        G: Generator<T> + 'static + std::panic::UnwindSafe,
        T: std::fmt::Debug + Send + 'static,
    {
        expect_panic(
            move || {
                Hegel::new(move |tc| {
                    tc.draw(&generator);
                })
                .settings(Settings::new().test_cases(1).database(None))
                .run();
            },
            pattern,
        );
    }

    #[test]
    fn test_nonexistent_category_argument() {
        expect_generator_panic(
            gs::characters().exclude_categories(&["foo"]),
            "(?i)(invalid|foo|categor|no valid)",
        );
    }

    #[test]
    fn test_bad_codepoint_arguments() {
        expect_generator_panic(
            gs::characters().min_codepoint(42).max_codepoint(24),
            "(?i)(invalid|min_codepoint|max_codepoint|no valid)",
        );
    }

    #[test]
    fn test_exclude_all_available_range() {
        expect_generator_panic(
            gs::characters()
                .min_codepoint(b'0' as u32)
                .max_codepoint(b'0' as u32)
                .exclude_characters("0"),
            "(?i)(invalid|no valid|empty)",
        );
    }

    #[test]
    fn test_when_nothing_could_be_produced() {
        expect_generator_panic(
            gs::characters()
                .categories(&["Cc"])
                .min_codepoint(b'0' as u32)
                .max_codepoint(b'9' as u32),
            "(?i)(invalid|no valid|empty)",
        );
    }

    #[test]
    fn test_find_one() {
        let c = minimal(
            gs::characters().min_codepoint(48).max_codepoint(48),
            |_: &char| true,
        );
        assert_eq!(c, '0');
    }

    #[test]
    fn test_whitelisted_characters_overlap_blacklisted_characters() {
        expect_generator_panic(
            gs::characters()
                .min_codepoint(b'0' as u32)
                .max_codepoint(b'9' as u32)
                .include_characters("te02тест49st")
                .exclude_characters("ts94тсет"),
            "(?i)(invalid|overlap|both)",
        );
    }

    #[test]
    fn test_whitelisted_characters_override() {
        let good = "teтестst";
        let good_owned = good.to_string();
        find_any(
            gs::characters()
                .min_codepoint(b'0' as u32)
                .max_codepoint(b'9' as u32)
                .include_characters(good),
            move |c: &char| good_owned.contains(*c),
        );
        find_any(
            gs::characters()
                .min_codepoint(b'0' as u32)
                .max_codepoint(b'9' as u32)
                .include_characters(good),
            |c: &char| "0123456789".contains(*c),
        );
        let combined = format!("{good}0123456789");
        assert_no_examples(
            gs::characters()
                .min_codepoint(b'0' as u32)
                .max_codepoint(b'9' as u32)
                .include_characters(good),
            move |c: &char| !combined.contains(*c),
        );
    }

    #[test]
    fn test_blacklisted_characters() {
        let bad = "te02тест49st";
        let c = minimal(
            gs::characters()
                .min_codepoint(b'0' as u32)
                .max_codepoint(b'9' as u32)
                .exclude_characters(bad),
            |_: &char| true,
        );
        assert_eq!(c, '1');

        let bad_owned = bad.to_string();
        assert_no_examples(
            gs::characters()
                .min_codepoint(b'0' as u32)
                .max_codepoint(b'9' as u32)
                .exclude_characters(bad),
            move |c: &char| bad_owned.contains(*c),
        );
    }

    #[test]
    fn test_whitelist_characters_disjoint_blacklist_characters() {
        let bad = "456def";
        let bad_owned = bad.to_string();
        assert_no_examples(
            gs::characters()
                .min_codepoint(b'0' as u32)
                .max_codepoint(b'9' as u32)
                .exclude_characters(bad)
                .include_characters("123abc"),
            move |c: &char| bad_owned.contains(*c),
        );
    }
}

mod nocover_characters {
    use crate::common::utils::assert_all_examples;
    use hegel::generators as gs;
    use hegel::{Hegel, Settings};

    const IDENTIFIER_CHARS: &str =
        "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789_";

    #[test]
    fn test_large_blacklist() {
        assert_all_examples(
            gs::characters().exclude_characters(IDENTIFIER_CHARS),
            |c: &char| !IDENTIFIER_CHARS.contains(*c),
        );
    }

    #[test]
    fn test_arbitrary_blacklist() {
        Hegel::new(|tc| {
            let blacklist: String = tc.draw(gs::text().max_codepoint(1000).min_size(1));
            let ords: Vec<u32> = blacklist.chars().map(|c| c as u32).collect();
            let min_cp = ords.iter().min().copied().unwrap().saturating_sub(1);
            let max_cp = ords.iter().max().copied().unwrap() + 1;
            let c: char = tc.draw(
                gs::characters()
                    .exclude_characters(&blacklist)
                    .min_codepoint(min_cp)
                    .max_codepoint(max_cp),
            );
            assert!(!blacklist.contains(c));
        })
        .settings(Settings::new().test_cases(100).database(None))
        .run();
    }
}

mod nocover_emails {
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
}

mod regex_tests {
    use crate::common::utils::{
        FindAny, assert_all_examples, assert_no_examples, check_can_generate_examples, find_any,
    };
    use hegel::generators::{self as gs};
    use hegel::{HealthCheck, Hegel, Settings};
    use regex::Regex;

    #[test]
    fn test_can_generate_patterns_no_alphabet() {
        for pattern in [
            ".",
            "a",
            "abc",
            "[a][b][c]",
            "[^a][^b][^c]",
            "[a-z0-9_]",
            "[^a-z0-9_]",
            "ab?",
            "ab*",
            "ab+",
            "ab{5}",
            "ab{5,10}",
            "ab{,10}",
            "ab{5,}",
            "ab|cd|ef",
            "(foo)+",
            r#"(['\"])[a-z]+\1"#,
            r#"(?:[a-z])(['\"])[a-z]+\1"#,
            r#"(?P<foo>['\"])[a-z]+(?P=foo)"#,
            "^abc",
            r"\d",
            r"[\d]",
            r"[^\D]",
            r"\w",
            r"[\w]",
            r"[^\W]",
            r"\s",
            r"[\s]",
            r"[^\S]",
        ] {
            check_can_generate_examples(gs::from_regex(pattern));
        }
    }

    #[test]
    fn test_can_generate_patterns_with_alphabet() {
        for pattern in [
            ".",
            "a",
            "abc",
            "[a][b][c]",
            "[^a][^b][^c]",
            "[a-z0-9_]",
            "[^a-z0-9_]",
            "ab?",
            "ab*",
            "ab+",
            "ab{5,10}",
            "ab|cd|ef",
            "(foo)+",
            r"\d",
            r"\w",
            r"\s",
        ] {
            check_can_generate_examples(
                gs::from_regex(pattern).alphabet(gs::characters().max_codepoint(1000)),
            );
        }
    }

    #[test]
    fn test_literals_with_ignorecase_a() {
        find_any(gs::from_regex(r"(?i)\Aa\Z"), |s: &String| s == "a");
        find_any(gs::from_regex(r"(?i)\Aa\Z"), |s: &String| s == "A");
    }

    #[test]
    fn test_literals_with_ignorecase_ab() {
        find_any(gs::from_regex(r"(?i)\A[ab]\Z"), |s: &String| s == "a");
        find_any(gs::from_regex(r"(?i)\A[ab]\Z"), |s: &String| s == "A");
    }

    #[test]
    fn test_not_literal_with_ignorecase() {
        assert_all_examples(gs::from_regex(r"(?i)\A[^a][^b]\Z"), |s: &String| {
            let mut chars = s.chars();
            let c0 = chars.next().unwrap();
            let c1 = chars.next().unwrap();
            c0 != 'a' && c0 != 'A' && c1 != 'b' && c1 != 'B'
        });
    }

    #[test]
    fn test_any_doesnt_generate_newline() {
        assert_all_examples(gs::from_regex(r"\A.\Z"), |s: &String| s != "\n");
    }

    #[test]
    fn test_any_with_dotall_generate_newline() {
        FindAny::new(gs::from_regex(r"(?s)\A.\Z"), |s: &String| s == "\n")
            .max_attempts(10_000)
            .run();
    }

    #[test]
    fn test_caret_in_the_middle_does_not_generate_anything() {
        assert_no_examples(gs::from_regex("a^b"), |_: &String| true);
    }

    #[test]
    fn test_end_with_terminator_does_not_pad() {
        assert_all_examples(gs::from_regex(r"abc\Z").fullmatch(false), |s: &String| {
            s.ends_with("abc")
        });
    }

    #[test]
    fn test_end() {
        find_any(gs::from_regex(r"\Aabc$").fullmatch(false), |s: &String| {
            s == "abc"
        });
        find_any(gs::from_regex(r"\Aabc$").fullmatch(false), |s: &String| {
            s == "abc\n"
        });
    }

    #[test]
    fn test_groupref_exists() {
        assert_all_examples(gs::from_regex("^(<)?a(?(1)>)$"), |s: &String| {
            ["a", "<a>"].contains(&s.as_str())
        });
        assert_all_examples(gs::from_regex("^(a)?(?(1)b|c)$"), |s: &String| {
            ["ab", "c"].contains(&s.as_str())
        });
    }

    #[test]
    fn test_text_with_large_min_size_and_no_max_still_varies_length() {
        assert_all_examples(gs::text().min_size(150), |s: &String| {
            (150..=250).contains(&s.chars().count())
        });
    }

    #[test]
    fn test_binary_with_large_min_size_and_no_max_still_varies_length() {
        assert_all_examples(gs::binary().min_size(150), |b: &Vec<u8>| {
            (150..=250).contains(&b.len())
        });
    }

    #[test]
    fn test_builder_calls_after_a_draw_are_not_ignored() {
        Hegel::new(|tc| {
            let g = gs::text();
            let _: String = tc.draw(&g);
            let g = g.max_size(2);
            let s: String = tc.draw(&g);
            assert!(
                s.chars().count() <= 2,
                "a builder call after a draw was ignored by the cached handle: {s:?}"
            );
        })
        .settings(Settings::new().database(None))
        .run();
    }

    #[test]
    fn test_word_boundaries_hold_in_generated_strings() {
        fn is_word(c: char) -> bool {
            c == '_' || c.is_alphanumeric()
        }
        assert_all_examples(gs::from_regex(r"\bfoo\b").fullmatch(false), |s: &String| {
            let cs: Vec<char> = s.chars().collect();
            (0..cs.len().saturating_sub(2)).any(|i| {
                cs[i..i + 3] == ['f', 'o', 'o']
                    && (i == 0 || !is_word(cs[i - 1]))
                    && (i + 3 == cs.len() || !is_word(cs[i + 3]))
            })
        });
    }

    #[test]
    fn test_impossible_negative_lookahead() {
        assert_no_examples(gs::from_regex("(?!foo)foo"), |_: &String| true);
    }

    #[test]
    fn test_impossible_negative_lookbehind() {
        assert_no_examples(
            gs::from_regex("abc(?<!abc)").fullmatch(true),
            |_: &String| true,
        );
    }

    #[test]
    fn test_can_handle_boundaries_nested() {
        Hegel::new(|tc| {
            let s: String = tc.draw(gs::from_regex(r"(\Afoo\Z)").fullmatch(false));
            assert_eq!(s, "foo");
        })
        .settings(Settings::new().database(None))
        .run();
    }

    #[test]
    fn test_groupref_not_shared_between_regex() {
        Hegel::new(|tc| {
            let _a: String = tc.draw(gs::from_regex(r"(a)\1"));
            let _b: String = tc.draw(gs::from_regex(r"(b)\1"));
        })
        .settings(Settings::new().database(None))
        .run();
    }

    #[test]
    fn test_positive_lookbehind() {
        FindAny::new(
            gs::from_regex(".*(?<=ab)c").fullmatch(false),
            |s: &String| s.ends_with("abc"),
        )
        .suppress_health_check(HealthCheck::TooSlow)
        .run();
    }

    #[test]
    fn test_positive_lookahead() {
        FindAny::new(
            gs::from_regex("a(?=bc).*").fullmatch(false),
            |s: &String| s.starts_with("abc"),
        )
        .suppress_health_check(HealthCheck::TooSlow)
        .run();
    }

    #[test]
    fn test_negative_lookbehind() {
        assert_all_examples(gs::from_regex("[abc]*(?<!abc)d"), |s: &String| {
            !s.ends_with("abcd")
        });
        assert_no_examples(gs::from_regex("[abc]*(?<!abc)d"), |s: &String| {
            s.ends_with("abcd")
        });
    }

    #[test]
    fn test_negative_lookahead() {
        assert_all_examples(gs::from_regex("^ab(?!cd)[abcd]*"), |s: &String| {
            !s.starts_with("abcd")
        });
        assert_no_examples(gs::from_regex("^ab(?!cd)[abcd]*"), |s: &String| {
            s.starts_with("abcd")
        });
    }

    #[test]
    fn test_generates_only_the_provided_characters_given_boundaries() {
        Hegel::new(|tc| {
            let xs: String = tc.draw(gs::from_regex(r"^a+\Z"));
            assert!(xs.chars().all(|c| c == 'a'));
        })
        .settings(Settings::new().database(None))
        .run();
    }

    #[test]
    fn test_group_backref_may_not_be_present() {
        Hegel::new(|tc| {
            let s: String = tc.draw(gs::from_regex(r"^(.)?\1\Z"));
            assert_eq!(s.chars().count(), 2);
            assert_eq!(s.chars().next(), s.chars().last());
        })
        .settings(Settings::new().database(None))
        .run();
    }

    #[test]
    fn test_subpattern_flags() {
        find_any(gs::from_regex(r"(?i)\Aa(?-i:b)\Z"), |s: &String| {
            s.starts_with('a')
        });
        find_any(gs::from_regex(r"(?i)\Aa(?-i:b)\Z"), |s: &String| {
            s.starts_with('A')
        });
        find_any(gs::from_regex(r"(?i)\Aa(?-i:b)\Z"), |s: &String| {
            s.chars().nth(1) == Some('b')
        });
        assert_no_examples(gs::from_regex(r"(?i)\Aa(?-i:b)\Z"), |s: &String| {
            s.chars().nth(1) == Some('B')
        });
    }

    #[test]
    fn test_can_pad_strings_arbitrarily() {
        find_any(gs::from_regex("a").fullmatch(false), |s: &String| {
            !s.starts_with('a')
        });
        find_any(gs::from_regex("a").fullmatch(false), |s: &String| {
            !s.ends_with('a')
        });
    }

    #[test]
    fn test_can_pad_empty_strings() {
        find_any(gs::from_regex("").fullmatch(false), |s: &String| {
            !s.is_empty()
        });
    }

    #[test]
    fn test_can_pad_strings_with_newlines() {
        find_any(gs::from_regex("^$").fullmatch(false), |s: &String| {
            !s.is_empty()
        });
    }

    #[test]
    fn test_given_multiline_regex_can_insert_after_dollar() {
        find_any(
            gs::from_regex(r"(?m)\Ahi$").fullmatch(false),
            |s: &String| s.contains('\n') && s.split('\n').nth(1).is_some_and(|p| !p.is_empty()),
        );
    }

    #[test]
    fn test_given_multiline_regex_can_insert_before_caret() {
        find_any(
            gs::from_regex(r"(?m)^hi\Z").fullmatch(false),
            |s: &String| s.contains('\n') && s.split('\n').next().is_some_and(|p| !p.is_empty()),
        );
    }

    #[test]
    fn test_does_not_left_pad_beginning_of_string_marker() {
        assert_all_examples(gs::from_regex(r"\Afoo").fullmatch(false), |s: &String| {
            s.starts_with("foo")
        });
    }

    #[test]
    fn test_bare_caret_can_produce() {
        find_any(gs::from_regex("^").fullmatch(false), |s: &String| {
            !s.is_empty()
        });
    }

    #[test]
    fn test_bare_dollar_can_produce() {
        find_any(gs::from_regex("$").fullmatch(false), |s: &String| {
            !s.is_empty()
        });
    }

    #[test]
    fn test_shared_union() {
        check_can_generate_examples(gs::from_regex(".|."));
    }

    #[test]
    fn test_issue_992_regression() {
        check_can_generate_examples(gs::from_regex(
            r"(?x)\d +  # the integral part
                \.    # the decimal point
                \d *  # some fractional digits",
        ));
    }

    #[test]
    fn test_fullmatch_is_the_default() {
        let re = Regex::new(r"\A[ab]+\z").unwrap();
        assert_all_examples(gs::from_regex("[ab]+"), move |s: &String| re.is_match(s));
    }

    #[test]
    fn test_fullmatch_generates_example_literal() {
        find_any(gs::from_regex("a").fullmatch(true), |s: &String| s == "a");
    }

    #[test]
    fn test_fullmatch_generates_example_charset() {
        find_any(gs::from_regex("[Aa]").fullmatch(true), |s: &String| {
            s == "A"
        });
    }

    #[test]
    fn test_fullmatch_generates_example_star() {
        find_any(gs::from_regex("[ab]*").fullmatch(true), |s: &String| {
            s == "abb"
        });
    }

    #[test]
    fn test_fullmatch_generates_example_ignorecase_charset() {
        FindAny::new(
            gs::from_regex(r"(?i)[ab]*").fullmatch(true),
            |s: &String| s == "aBb",
        )
        .max_attempts(10_000)
        .run();
    }

    #[test]
    fn test_fullmatch_generates_example_ignorecase_single() {
        find_any(gs::from_regex(r"(?i)[ab]").fullmatch(true), |s: &String| {
            s == "A"
        });
    }

    #[test]
    fn test_fullmatch_matches_empty() {
        assert_all_examples(gs::from_regex("").fullmatch(true), |s: &String| {
            Regex::new(r"\A\z").unwrap().is_match(s)
        });
    }

    #[test]
    fn test_fullmatch_matches_comment() {
        assert_all_examples(
            gs::from_regex("(?#comment)").fullmatch(true),
            |s: &String| Regex::new(r"\A\z").unwrap().is_match(s),
        );
    }

    #[test]
    fn test_fullmatch_matches_literal_a() {
        assert_all_examples(gs::from_regex("a").fullmatch(true), |s: &String| {
            Regex::new(r"\Aa\z").unwrap().is_match(s)
        });
    }

    #[test]
    fn test_fullmatch_matches_charset_aa() {
        assert_all_examples(gs::from_regex("[Aa]").fullmatch(true), |s: &String| {
            Regex::new(r"\A[Aa]\z").unwrap().is_match(s)
        });
    }

    #[test]
    fn test_fullmatch_matches_star() {
        assert_all_examples(gs::from_regex("[ab]*").fullmatch(true), |s: &String| {
            Regex::new(r"\A[ab]*\z").unwrap().is_match(s)
        });
    }

    #[test]
    fn test_fullmatch_matches_ignorecase_star() {
        let re = Regex::new(r"(?i)\A[ab]*\z").unwrap();
        assert_all_examples(
            gs::from_regex(r"(?i)[ab]*").fullmatch(true),
            move |s: &String| re.is_match(s),
        );
    }

    #[test]
    fn test_fullmatch_matches_ignorecase_single() {
        let re = Regex::new(r"(?i)\A[ab]\z").unwrap();
        assert_all_examples(
            gs::from_regex(r"(?i)[ab]").fullmatch(true),
            move |s: &String| re.is_match(s),
        );
    }

    #[test]
    fn test_issue_1786_regression() {
        check_can_generate_examples(gs::from_regex(r"(?i)\\"));
    }

    #[test]
    fn test_sets_allow_multichar_output_in_ignorecase_mode() {
        find_any(
            gs::from_regex("(?i)[\u{0130}_]").fullmatch(false),
            |s: &String| s.chars().count() > 1,
        );
    }

    #[test]
    fn lookbehind_can_match_not_literal() {
        check_can_generate_examples(gs::from_regex(r"a(?<![^a])b"));
    }

    #[test]
    fn lookahead_with_any() {
        check_can_generate_examples(gs::from_regex("a(?!.x)b"));
    }

    #[test]
    fn lookahead_with_set() {
        check_can_generate_examples(gs::from_regex("x(?![abc])y"));
    }

    #[test]
    fn lookahead_with_set_ranges_and_ignorecase() {
        check_can_generate_examples(gs::from_regex(r"(?i)x(?![a-z])Y"));
    }

    #[test]
    fn lookahead_with_set_category() {
        check_can_generate_examples(gs::from_regex(r"x(?!\d)y"));
    }

    #[test]
    fn lookahead_with_anchor() {
        check_can_generate_examples(gs::from_regex(r"abc(?!\Z)").fullmatch(false));
    }

    #[test]
    fn lookahead_with_word_boundary() {
        check_can_generate_examples(gs::from_regex(r"abc(?!\b)d"));
    }

    #[test]
    fn lookahead_with_branch() {
        check_can_generate_examples(gs::from_regex("x(?!a|b)y"));
    }

    #[test]
    fn lookahead_with_subpattern() {
        check_can_generate_examples(gs::from_regex("x(?!(?:a))y"));
    }

    #[test]
    fn lookahead_with_atomic_group() {
        check_can_generate_examples(gs::from_regex("x(?!(?>a))y"));
    }

    #[test]
    fn lookahead_with_groupref() {
        check_can_generate_examples(gs::from_regex(r"(a)x(?!\1)y"));
    }

    #[test]
    fn lookahead_with_conditional_backref() {
        check_can_generate_examples(gs::from_regex(r"(a)?x(?!(?(1)a|b))y"));
    }

    #[test]
    fn lookahead_with_nested_positive_lookaround() {
        check_can_generate_examples(gs::from_regex(r"x(?!(?=a))y"));
    }

    #[test]
    fn lookahead_with_nested_negative_lookaround() {
        check_can_generate_examples(gs::from_regex(r"x(?!(?!a))y"));
    }

    #[test]
    fn lookahead_with_failure() {
        check_can_generate_examples(gs::from_regex(r"x(?!(?!))y"));
    }

    #[test]
    fn lookahead_with_max_repeat() {
        check_can_generate_examples(gs::from_regex(r"y(?!a*x)z"));
    }

    #[test]
    fn lookahead_with_min_repeat() {
        check_can_generate_examples(gs::from_regex(r"y(?!a*?x)z"));
    }

    #[test]
    fn lookahead_with_possessive_repeat() {
        check_can_generate_examples(gs::from_regex(r"y(?!a*+x)z"));
    }

    #[test]
    fn lookahead_with_multiline_anchor() {
        check_can_generate_examples(gs::from_regex(r"(?m)x(?!^)y"));
    }

    #[test]
    fn lookahead_with_dotall_dot() {
        check_can_generate_examples(gs::from_regex(r"(?s)x(?!.)y"));
    }

    #[test]
    fn lookahead_groupref_with_ignorecase() {
        check_can_generate_examples(gs::from_regex(r"(?i)(a)x(?!\1)y"));
    }

    #[test]
    fn ascii_flag_in_set_with_alphabet() {
        check_can_generate_examples(
            gs::from_regex(r"(?a)\w")
                .alphabet(gs::characters().min_codepoint(0).max_codepoint(0x300)),
        );
    }

    #[test]
    fn ascii_flag_in_negated_set_with_alphabet() {
        check_can_generate_examples(
            gs::from_regex(r"(?a)[^a]")
                .alphabet(gs::characters().min_codepoint(0).max_codepoint(0x300)),
        );
    }

    #[test]
    fn ignorecase_with_restricted_alphabet() {
        check_can_generate_examples(
            gs::from_regex(r"(?i)abc").alphabet(gs::characters().max_codepoint(0x7F)),
        );
    }

    #[test]
    fn not_literal_ignorecase_with_alphabet() {
        check_can_generate_examples(
            gs::from_regex(r"(?i)[^a]").alphabet(gs::characters().max_codepoint(0x7F)),
        );
    }

    #[test]
    fn atomic_group_with_alphabet() {
        check_can_generate_examples(
            gs::from_regex(r"(?>a)").alphabet(gs::characters().max_codepoint(0x7F)),
        );
    }

    #[test]
    fn min_repeat_with_alphabet() {
        check_can_generate_examples(
            gs::from_regex(r"a*?").alphabet(gs::characters().max_codepoint(0x7F)),
        );
    }

    #[test]
    fn possessive_repeat_with_alphabet() {
        check_can_generate_examples(
            gs::from_regex(r"a*+").alphabet(gs::characters().max_codepoint(0x7F)),
        );
    }

    #[test]
    fn fullmatch_with_literal_in_alphabet() {
        check_can_generate_examples(
            gs::from_regex(r"a").fullmatch(false).alphabet(
                gs::characters()
                    .min_codepoint(b'a' as u32)
                    .max_codepoint(b'a' as u32),
            ),
        );
    }

    #[test]
    fn literal_outside_alphabet_is_rejected_but_retried() {
        check_can_generate_examples(
            gs::from_regex(r"a?").alphabet(
                gs::characters()
                    .min_codepoint(b'b' as u32)
                    .max_codepoint(b'z' as u32),
            ),
        );
    }

    #[test]
    fn ignorecase_literal_swapcase_outside_alphabet() {
        check_can_generate_examples(
            gs::from_regex(r"(?i)A?").alphabet(
                gs::characters()
                    .min_codepoint(b'A' as u32)
                    .max_codepoint(b'Z' as u32),
            ),
        );
    }

    #[test]
    fn anchor_beginning_after_content() {
        check_can_generate_examples(gs::from_regex(r".*\A").fullmatch(false));
    }

    #[test]
    fn explicit_failure_pattern() {
        check_can_generate_examples(gs::from_regex(r"(?!)?"));
    }

    #[test]
    fn ascii_flag_positive_set_with_nonascii_literal() {
        check_can_generate_examples(gs::from_regex("(?a)[\u{0080}]?"));
    }

    #[test]
    fn positive_set_outside_alphabet() {
        check_can_generate_examples(
            gs::from_regex(r"[a-z]?").alphabet(
                gs::characters()
                    .min_codepoint(b'A' as u32)
                    .max_codepoint(b'Z' as u32),
            ),
        );
    }

    #[test]
    fn ascii_flag_negated_set_with_nonascii_alphabet() {
        check_can_generate_examples(
            gs::from_regex(r"(?a)[^a]?")
                .alphabet(gs::characters().min_codepoint(0x100).max_codepoint(0x200)),
        );
    }

    #[test]
    fn padded_pattern_with_empty_alphabet_intervals() {
        check_can_generate_examples(
            gs::from_regex(r"a?")
                .fullmatch(false)
                .alphabet(gs::characters().categories(&[])),
        );
    }

    #[test]
    fn anchor_at_start_after_content() {
        check_can_generate_examples(gs::from_regex(r"\Aabc").fullmatch(false));
    }

    #[test]
    fn anchor_multiline_with_padding() {
        check_can_generate_examples(gs::from_regex(r"(?m)^abc").fullmatch(false));
    }
}

mod nocover_bad_repr {
    use crate::common::utils::check_can_generate_examples;
    use hegel::generators as gs;

    #[test]
    fn test_sampled_from_bad_repr() {
        check_can_generate_examples(gs::sampled_from(vec![
            "✐", "✑", "✒", "✓", "✔", "✕", "✖", "✗", "✘", "✙", "✚", "✛", "✜", "✝", "✞", "✟", "✠",
            "✡", "✢", "✣",
        ]));
    }
}
