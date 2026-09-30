//! Standard base64 (RFC 4648), for data URLs and embedded fonts.

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Encodes bytes as padded base64.
pub(crate) fn encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = chunk
            .iter()
            .enumerate()
            .fold(0u32, |n, (i, b)| n | u32::from(*b) << (16 - 8 * i));
        for i in 0..4 {
            out.push(if i <= chunk.len() {
                char::from(ALPHABET[(n >> (18 - 6 * i) & 0x3f) as usize])
            } else {
                '='
            });
        }
    }
    out
}

/// Decodes base64, ignoring whitespace and padding. Returns `None` on any
/// other character.
pub(crate) fn decode(text: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(text.len() / 4 * 3);
    let (mut bits, mut count) = (0u32, 0u32);
    for byte in text
        .bytes()
        .filter(|b| !b.is_ascii_whitespace() && *b != b'=')
    {
        let value = ALPHABET.iter().position(|a| *a == byte)? as u32;
        bits = bits << 6 | value;
        count += 6;
        if count >= 8 {
            count -= 8;
            out.push((bits >> count & 0xff) as u8);
        }
    }
    Some(out)
}

/// Splits a `data:<mime>;base64,<payload>` URL into its type and bytes.
pub(crate) fn decode_data_url(url: &str) -> Option<(&str, Vec<u8>)> {
    let rest = url.strip_prefix("data:")?;
    let (header, payload) = rest.split_once(',')?;
    let mime = header.strip_suffix(";base64")?;
    Some((mime, decode(payload)?))
}

#[cfg(test)]
mod tests {
    use super::{decode, decode_data_url, encode};

    #[test]
    fn round_trips_and_matches_rfc_4648() {
        for (plain, coded) in [
            ("", ""),
            ("M", "TQ=="),
            ("Ma", "TWE="),
            ("Man", "TWFu"),
            ("hello!", "aGVsbG8h"),
        ] {
            assert_eq!(encode(plain.as_bytes()), coded);
            assert_eq!(decode(coded).unwrap(), plain.as_bytes());
        }
        let bytes: Vec<u8> = (0..=255).collect();
        assert_eq!(decode(&encode(&bytes)).unwrap(), bytes);
        assert_eq!(decode("a$b"), None);
        assert_eq!(
            decode_data_url("data:image/png;base64,TWFu"),
            Some(("image/png", b"Man".to_vec()))
        );
        assert_eq!(decode_data_url("https://example.com/a.png"), None);
    }
}
