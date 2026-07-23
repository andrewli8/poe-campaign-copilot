//! Base64(zlib) share-code decoding, with raw-XML passthrough.
//!
//! PoB share codes are `URL-safe base64(zlib(xml))`. Sites that host codes
//! (pobb.in, pastebin mirrors, ...) are out of scope here — a pasted URL is
//! rejected with a clear error rather than silently failing to decode.

use std::io::Read;

use base64::Engine as _;
use flate2::read::ZlibDecoder;

use crate::PobError;

/// Hard cap on inflated (decompressed) XML size. PoB builds are a few KB to
/// a few hundred KB of XML; zlib's compression ratio means a small,
/// maliciously (or accidentally, e.g. a corrupt paste) crafted input could
/// otherwise inflate to gigabytes and exhaust memory. 64 MiB is generous
/// headroom over any real build while still bounding the worst case.
const MAX_INFLATED_BYTES: u64 = 64 * 1024 * 1024;

/// Hard cap on the raw pasted input itself (share code OR raw XML), checked
/// before any decoding work happens. `MAX_INFLATED_BYTES` only bounds the
/// zlib-inflate path's *output*; a giant raw-XML paste (the `starts_with('<')`
/// passthrough below) or a giant base64 blob would otherwise be processed in
/// full before any size check ever runs. 2 MiB is generous headroom over any
/// real PoB export (a few hundred KB of XML at most).
const MAX_INPUT_BYTES: usize = 2 * 1024 * 1024;

pub fn decode_share_code(code: &str) -> Result<String, PobError> {
    let trimmed = code.trim();

    if trimmed.is_empty() {
        return Err(PobError::NotACode);
    }
    if trimmed.len() > MAX_INPUT_BYTES {
        return Err(PobError::Decode("input too large".to_string()));
    }
    if trimmed.starts_with("http") {
        return Err(PobError::UrlNotSupported);
    }
    if trimmed.starts_with('<') {
        // Raw XML passthrough: return the input as-is (not the trimmed
        // copy), so callers get back exactly what they handed in.
        return Ok(code.to_string());
    }

    // Real share codes get line-wrapped when pasted through Discord,
    // pastebin-style sites, etc, so strip ALL ASCII whitespace — not just
    // the leading/trailing whitespace `trim()` above already removed — before
    // treating what's left as base64. This runs only on the base64 path:
    // the `<`/`http` prefix checks above already ran against `trimmed`, so a
    // whitespace-mangled URL or XML document is still classified correctly.
    let stripped: String = trimmed
        .chars()
        .filter(|c| !c.is_ascii_whitespace())
        .collect();

    let bytes = decode_base64(&stripped)?;
    inflate(&bytes)
}

/// PoB encodes with the padded URL-safe alphabet, but some export tools
/// (and users copy-pasting) strip the trailing `=` padding, so try both.
fn decode_base64(code: &str) -> Result<Vec<u8>, PobError> {
    base64::engine::general_purpose::URL_SAFE
        .decode(code)
        .or_else(|_| base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(code))
        .map_err(|e| PobError::Decode(e.to_string()))
}

fn inflate(bytes: &[u8]) -> Result<String, PobError> {
    let decoder = ZlibDecoder::new(bytes);
    let buf = read_capped(decoder, MAX_INFLATED_BYTES)?;

    let xml = String::from_utf8(buf).map_err(|e| PobError::Decode(e.to_string()))?;
    if !xml.trim_start().starts_with('<') {
        return Err(PobError::Decode(
            "decoded content does not look like XML".to_string(),
        ));
    }

    Ok(xml)
}

/// Reads all of `reader` into a `Vec<u8>`, capped at `limit` bytes: reads at
/// most `limit + 1` bytes (via `Read::take`), then errors if that many were
/// actually produced — which can only happen if the underlying stream had
/// more than `limit` bytes left, i.e. the cap was exceeded. A stream that
/// ends at or before `limit` bytes reads through normally. Factored out
/// (rather than inlined into `inflate`) so the cap logic is unit-testable
/// without needing a real zlib stream that inflates past the limit.
fn read_capped<R: Read>(reader: R, limit: u64) -> Result<Vec<u8>, PobError> {
    let mut limited = reader.take(limit + 1);
    let mut buf = Vec::new();
    limited
        .read_to_end(&mut buf)
        .map_err(|e| PobError::Decode(e.to_string()))?;
    if buf.len() as u64 > limit {
        return Err(PobError::Decode("build too large".to_string()));
    }
    Ok(buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_internal_whitespace_before_base64_decode() {
        use base64::Engine as _;
        use flate2::{Compression, write::ZlibEncoder};
        use std::io::Write as _;

        let xml = "<PathOfBuilding><Build className=\"Witch\"/></PathOfBuilding>";
        let mut e = ZlibEncoder::new(Vec::new(), Compression::default());
        e.write_all(xml.as_bytes()).unwrap();
        let code = base64::engine::general_purpose::URL_SAFE.encode(e.finish().unwrap());

        // Simulate Discord/pastebin line-wrapping: inject a mix of
        // newlines, a carriage return, a tab, and a stray space at various
        // points in the middle of the code, not just at the ends.
        let mut wrapped = String::new();
        for (i, ch) in code.chars().enumerate() {
            wrapped.push(ch);
            match i % 11 {
                3 => wrapped.push('\n'),
                5 => wrapped.push('\r'),
                7 => wrapped.push('\t'),
                9 => wrapped.push(' '),
                _ => {}
            }
        }
        wrapped.push('\n'); // trailing wrap too

        assert_eq!(decode_share_code(&wrapped).unwrap(), xml);
        assert_eq!(decode_share_code(&code).unwrap(), xml);
    }

    #[test]
    fn read_capped_passes_through_stream_at_or_under_limit() {
        let data = vec![7u8; 10];
        let out = read_capped(std::io::Cursor::new(data.clone()), 10).unwrap();
        assert_eq!(out, data);
    }

    #[test]
    fn read_capped_errors_when_stream_exceeds_limit() {
        let data = vec![7u8; 11];
        let err = read_capped(std::io::Cursor::new(data), 10).unwrap_err();
        assert!(matches!(err, PobError::Decode(msg) if msg.contains("too large")));
    }

    #[test]
    fn decode_share_code_rejects_input_over_the_size_cap_before_any_decoding() {
        // 3 MiB of plausible-looking base64 characters, well over
        // MAX_INPUT_BYTES (2 MiB) and also well over MAX_INFLATED_BYTES
        // would be if this ever reached the inflate path — proving the cap
        // is enforced up front, not merely inherited from the inflate cap.
        let huge = "A".repeat(3 * 1024 * 1024);
        let err = decode_share_code(&huge).unwrap_err();
        assert!(matches!(err, PobError::Decode(msg) if msg.contains("too large")));
    }

    #[test]
    fn decode_share_code_rejects_oversized_raw_xml_passthrough_too() {
        // The raw-XML passthrough branch (starts_with('<')) must also be
        // bounded by the same up-front cap, not just the base64/inflate
        // path.
        let huge_xml = format!("<{}", "a".repeat(3 * 1024 * 1024));
        let err = decode_share_code(&huge_xml).unwrap_err();
        assert!(matches!(err, PobError::Decode(msg) if msg.contains("too large")));
    }

    #[test]
    fn decode_share_code_still_works_for_a_normal_sized_code() {
        use base64::Engine as _;
        use flate2::{Compression, write::ZlibEncoder};
        use std::io::Write as _;

        let xml = "<PathOfBuilding><Build className=\"Witch\"/></PathOfBuilding>";
        let mut e = ZlibEncoder::new(Vec::new(), Compression::default());
        e.write_all(xml.as_bytes()).unwrap();
        let code = base64::engine::general_purpose::URL_SAFE.encode(e.finish().unwrap());

        assert_eq!(decode_share_code(&code).unwrap(), xml);
    }
}
