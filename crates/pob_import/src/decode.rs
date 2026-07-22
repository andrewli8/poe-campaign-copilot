//! Base64(zlib) share-code decoding, with raw-XML passthrough.
//!
//! PoB share codes are `URL-safe base64(zlib(xml))`. Sites that host codes
//! (pobb.in, pastebin mirrors, ...) are out of scope here — a pasted URL is
//! rejected with a clear error rather than silently failing to decode.

use std::io::Read as _;

use base64::Engine as _;
use flate2::read::ZlibDecoder;

use crate::PobError;

pub fn decode_share_code(code: &str) -> Result<String, PobError> {
    let trimmed = code.trim();

    if trimmed.is_empty() {
        return Err(PobError::NotACode);
    }
    if trimmed.starts_with("http") {
        return Err(PobError::UrlNotSupported);
    }
    if trimmed.starts_with('<') {
        // Raw XML passthrough: return the input as-is (not the trimmed
        // copy), so callers get back exactly what they handed in.
        return Ok(code.to_string());
    }

    let bytes = decode_base64(trimmed)?;
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
    let mut decoder = ZlibDecoder::new(bytes);
    let mut xml = String::new();
    decoder
        .read_to_string(&mut xml)
        .map_err(|e| PobError::Decode(e.to_string()))?;

    if !xml.trim_start().starts_with('<') {
        return Err(PobError::Decode(
            "decoded content does not look like XML".to_string(),
        ));
    }

    Ok(xml)
}
