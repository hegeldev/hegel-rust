/// Standard base64 alphabet (RFC 4648).
const B64_ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Encode bytes as a padded standard-alphabet base64 string.
pub(crate) fn base64_encode(data: &[u8]) -> String {
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b0 = u32::from(chunk[0]);
        let b1 = chunk.get(1).copied().map_or(0, u32::from);
        let b2 = chunk.get(2).copied().map_or(0, u32::from);
        let n = (b0 << 16) | (b1 << 8) | b2;
        out.push(B64_ALPHABET[((n >> 18) & 0x3f) as usize] as char);
        out.push(B64_ALPHABET[((n >> 12) & 0x3f) as usize] as char);
        out.push(if chunk.len() > 1 {
            B64_ALPHABET[((n >> 6) & 0x3f) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            B64_ALPHABET[(n & 0x3f) as usize] as char
        } else {
            '='
        });
    }
    out
}

/// Decode a single base64 digit to its 6-bit value, or `None` for any byte
/// outside the standard alphabet (including `=`, handled separately).
fn base64_value(c: u8) -> Option<u8> {
    match c {
        b'A'..=b'Z' => Some(c - b'A'),
        b'a'..=b'z' => Some(c - b'a' + 26),
        b'0'..=b'9' => Some(c - b'0' + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    }
}

/// Decode a padded standard-alphabet base64 string, or `None` if the input
/// length isn't a multiple of 4, contains a non-alphabet byte, or uses `=`
/// padding anywhere but the tail of the final quad.
pub(crate) fn base64_decode(s: &str) -> Option<Vec<u8>> {
    let s = s.as_bytes();
    if s.len() % 4 != 0 {
        return None;
    }
    let n_chunks = s.len() / 4;
    let mut out = Vec::with_capacity(n_chunks * 3);
    for (i, chunk) in s.chunks(4).enumerate() {
        let last = i + 1 == n_chunks;
        let c0 = base64_value(chunk[0])?;
        let c1 = base64_value(chunk[1])?;
        let pad2 = chunk[2] == b'=';
        let pad3 = chunk[3] == b'=';
        if (pad2 || pad3) && !last {
            return None;
        }
        if pad2 && !pad3 {
            return None;
        }
        let c2 = if pad2 { 0 } else { base64_value(chunk[2])? };
        let c3 = if pad3 { 0 } else { base64_value(chunk[3])? };
        let n =
            (u32::from(c0) << 18) | (u32::from(c1) << 12) | (u32::from(c2) << 6) | u32::from(c3);
        out.push((n >> 16) as u8);
        if !pad2 {
            out.push((n >> 8) as u8);
        }
        if !pad3 {
            out.push(n as u8);
        }
    }
    Some(out)
}

#[cfg(test)]
#[path = "../../tests/embedded/native/base64_tests.rs"]
mod tests;
