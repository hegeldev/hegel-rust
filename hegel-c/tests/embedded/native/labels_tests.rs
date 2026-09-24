use super::*;
use crate::native::draws;

#[test]
fn label_from_name_hashes_the_utf8_bytes() {
    assert_eq!(label_from_name(""), 0xcbf29ce484222325);
    assert_eq!(label_from_name("a"), 0xaf63dc4c8601ec8c);
    assert_eq!(label_from_name("foobar"), 0x85944171f73967e8);
    assert_eq!(
        label_from_name("hegel.integer"),
        label_from_bytes(b"hegel.integer")
    );
    assert_eq!(
        label_from_name("ünïcödé"),
        label_from_bytes("ünïcödé".as_bytes())
    );
}

#[test]
fn the_engines_own_labels_are_distinct() {
    let mut labels = alloc::vec![
        draws::LABEL_REGEX,
        draws::LABEL_EMAIL,
        draws::LABEL_URL,
        draws::LABEL_DOMAIN,
        draws::LABEL_DATE,
        draws::LABEL_TIME,
        draws::LABEL_DATETIME,
        draws::LABEL_UUID,
        draws::LABEL_IP_ADDRESS,
        draws::LABEL_INTEGER,
        draws::LABEL_FLOAT,
        draws::LABEL_BOOLEAN,
        draws::LABEL_BYTES,
        draws::LABEL_STRING,
        draws::LABEL_FRESH_ID,
        draws::LABEL_SET_CHOICE,
        draws::LABEL_CONCURRENCY,
        draws::LABEL_FEATURE_FLAG,
    ];
    let count = labels.len();
    labels.sort_unstable();
    labels.dedup();
    assert_eq!(labels.len(), count);
}

#[test]
fn combining_is_order_sensitive_and_never_the_identity() {
    let a = label_from_name("a");
    let b = label_from_name("b");
    assert_ne!(combine_labels(&[a, b]), combine_labels(&[b, a]));
    assert_ne!(combine_labels(&[a]), a);
    assert_ne!(combine_labels(&[a]), combine_labels(&[a, a]));
    assert_eq!(combine_labels(&[]), 0xcbf29ce484222325);
}
