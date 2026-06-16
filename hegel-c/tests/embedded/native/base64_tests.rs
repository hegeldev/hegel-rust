// Embedded tests for src/native/base64.rs — the inline base64 codec.

use super::*;

#[test]
fn round_trips_all_byte_values_and_lengths() {
    // Covers every alphabet range plus all three trailing-chunk paddings
    // (len % 3 == 0, 1, 2).
    for len in [0usize, 1, 2, 3, 255, 256] {
        let data: Vec<u8> = (0..len).map(|i| (i % 256) as u8).collect();
        let encoded = base64_encode(&data);
        assert_eq!(base64_decode(&encoded).unwrap(), data, "len={len}");
    }
}

#[test]
fn decode_rejects_length_not_multiple_of_four() {
    assert!(base64_decode("abc").is_none());
}

#[test]
fn decode_rejects_non_alphabet_byte() {
    assert!(base64_decode("ab*=").is_none());
}

#[test]
fn decode_rejects_padding_outside_the_final_quad() {
    // Padding in a non-final quad is invalid.
    assert!(base64_decode("AB==CDEF").is_none());
}

#[test]
fn decode_rejects_a_lone_third_position_pad() {
    // "=X" — a padded third position forces a padded fourth.
    assert!(base64_decode("AB=C").is_none());
}
