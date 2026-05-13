//! Burp-style decoder utilities.
//!
//! These are pure, deterministic transformations used by the Decoder tab and
//! also exposed as helpers to the Inspector for "decode this field".

use std::io::Read;

use base64::Engine;
use serde::{Deserialize, Serialize};

use crate::error::{NyxError, NyxResult};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Codec {
    Base64,
    Base64Url,
    Url,
    Html,
    Hex,
    Ascii,
    Gzip,
    Deflate,
    Zstd,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecoderResult {
    pub codec: Codec,
    pub success: bool,
    pub output: String,
}

pub fn encode(codec: Codec, input: &str) -> NyxResult<String> {
    match codec {
        Codec::Base64 => Ok(base64::engine::general_purpose::STANDARD.encode(input.as_bytes())),
        Codec::Base64Url => {
            Ok(base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(input.as_bytes()))
        }
        Codec::Url => Ok(url_encode(input)),
        Codec::Html => Ok(html_encode(input)),
        Codec::Hex => Ok(hex_encode(input.as_bytes())),
        Codec::Ascii => Ok(input.chars().map(|c| (c as u32).to_string()).collect::<Vec<_>>().join(" ")),
        Codec::Gzip => Ok(base64::engine::general_purpose::STANDARD.encode(gzip_encode(input.as_bytes())?)),
        Codec::Deflate => {
            Ok(base64::engine::general_purpose::STANDARD.encode(deflate_encode(input.as_bytes())?))
        }
        Codec::Zstd => Ok(base64::engine::general_purpose::STANDARD.encode(zstd_encode(input.as_bytes())?)),
    }
}

pub fn decode(codec: Codec, input: &str) -> NyxResult<String> {
    match codec {
        Codec::Base64 => {
            let raw = base64::engine::general_purpose::STANDARD
                .decode(input.trim().as_bytes())
                .map_err(|e| NyxError::Decode(format!("base64: {e}")))?;
            Ok(String::from_utf8_lossy(&raw).into_owned())
        }
        Codec::Base64Url => {
            let trimmed = input.trim();
            let raw = base64::engine::general_purpose::URL_SAFE_NO_PAD
                .decode(trimmed.as_bytes())
                .or_else(|_| {
                    base64::engine::general_purpose::URL_SAFE.decode(trimmed.as_bytes())
                })
                .map_err(|e| NyxError::Decode(format!("base64url: {e}")))?;
            Ok(String::from_utf8_lossy(&raw).into_owned())
        }
        Codec::Url => Ok(url_decode(input)),
        Codec::Html => Ok(html_decode(input)),
        Codec::Hex => {
            let bytes = hex_decode(input)?;
            Ok(String::from_utf8_lossy(&bytes).into_owned())
        }
        Codec::Ascii => Ok(input
            .split_whitespace()
            .filter_map(|token| token.parse::<u32>().ok())
            .filter_map(char::from_u32)
            .collect::<String>()),
        Codec::Gzip => {
            let raw = base64::engine::general_purpose::STANDARD
                .decode(input.trim().as_bytes())
                .map_err(|e| NyxError::Decode(format!("gzip-b64: {e}")))?;
            let plain = gzip_decode(&raw)?;
            Ok(String::from_utf8_lossy(&plain).into_owned())
        }
        Codec::Deflate => {
            let raw = base64::engine::general_purpose::STANDARD
                .decode(input.trim().as_bytes())
                .map_err(|e| NyxError::Decode(format!("deflate-b64: {e}")))?;
            let plain = deflate_decode(&raw)?;
            Ok(String::from_utf8_lossy(&plain).into_owned())
        }
        Codec::Zstd => {
            let raw = base64::engine::general_purpose::STANDARD
                .decode(input.trim().as_bytes())
                .map_err(|e| NyxError::Decode(format!("zstd-b64: {e}")))?;
            let plain = zstd_decode(&raw)?;
            Ok(String::from_utf8_lossy(&plain).into_owned())
        }
    }
}

pub fn smart_decode(input: &str) -> Vec<DecoderResult> {
    let mut out = Vec::new();
    for codec in [
        Codec::Base64,
        Codec::Base64Url,
        Codec::Url,
        Codec::Html,
        Codec::Hex,
        Codec::Gzip,
        Codec::Deflate,
        Codec::Zstd,
    ] {
        match decode(codec, input) {
            Ok(output) if !output.is_empty() => out.push(DecoderResult {
                codec,
                success: true,
                output,
            }),
            _ => {}
        }
    }
    out
}

fn url_encode(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for byte in input.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

fn url_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b == b'%' && i + 2 < bytes.len() {
            let hi = (bytes[i + 1] as char).to_digit(16);
            let lo = (bytes[i + 2] as char).to_digit(16);
            if let (Some(h), Some(l)) = (hi, lo) {
                out.push((h * 16 + l) as u8);
                i += 3;
                continue;
            }
        }
        if b == b'+' {
            out.push(b' ');
        } else {
            out.push(b);
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn html_encode(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for c in input.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

fn html_decode(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let bytes = input.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'&' {
            if let Some(end) = input[i..].find(';').map(|e| i + e) {
                let entity = &input[i + 1..end];
                let replacement = match entity {
                    "amp" => Some('&'),
                    "lt" => Some('<'),
                    "gt" => Some('>'),
                    "quot" => Some('"'),
                    "apos" | "#39" => Some('\''),
                    "nbsp" => Some(' '),
                    _ if entity.starts_with("#x") || entity.starts_with("#X") => entity[2..]
                        .chars()
                        .next()
                        .and(u32::from_str_radix(&entity[2..], 16).ok().and_then(char::from_u32)),
                    _ if entity.starts_with('#') => {
                        entity[1..].parse::<u32>().ok().and_then(char::from_u32)
                    }
                    _ => None,
                };
                if let Some(c) = replacement {
                    out.push(c);
                    i = end + 1;
                    continue;
                }
            }
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    out
}

fn hex_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push_str(&format!("{b:02x}"));
    }
    out
}

fn hex_decode(input: &str) -> NyxResult<Vec<u8>> {
    let cleaned: String = input.chars().filter(|c| !c.is_whitespace()).collect();
    if cleaned.len() % 2 != 0 {
        return Err(NyxError::Decode("hex input has odd length".into()));
    }
    let mut out = Vec::with_capacity(cleaned.len() / 2);
    for chunk in cleaned.as_bytes().chunks(2) {
        let chunk_str = std::str::from_utf8(chunk)
            .map_err(|e| NyxError::Decode(format!("hex utf8: {e}")))?;
        let byte = u8::from_str_radix(chunk_str, 16)
            .map_err(|e| NyxError::Decode(format!("hex parse: {e}")))?;
        out.push(byte);
    }
    Ok(out)
}

fn gzip_encode(input: &[u8]) -> NyxResult<Vec<u8>> {
    use flate2::write::GzEncoder;
    use flate2::Compression;
    use std::io::Write;

    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder
        .write_all(input)
        .map_err(|e| NyxError::Decode(format!("gzip encode: {e}")))?;
    encoder
        .finish()
        .map_err(|e| NyxError::Decode(format!("gzip finish: {e}")))
}

fn gzip_decode(input: &[u8]) -> NyxResult<Vec<u8>> {
    use flate2::read::GzDecoder;
    let mut decoder = GzDecoder::new(input);
    let mut out = Vec::new();
    decoder
        .read_to_end(&mut out)
        .map_err(|e| NyxError::Decode(format!("gzip decode: {e}")))?;
    Ok(out)
}

fn deflate_encode(input: &[u8]) -> NyxResult<Vec<u8>> {
    use flate2::write::ZlibEncoder;
    use flate2::Compression;
    use std::io::Write;
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
    encoder
        .write_all(input)
        .map_err(|e| NyxError::Decode(format!("deflate encode: {e}")))?;
    encoder
        .finish()
        .map_err(|e| NyxError::Decode(format!("deflate finish: {e}")))
}

fn deflate_decode(input: &[u8]) -> NyxResult<Vec<u8>> {
    use flate2::read::ZlibDecoder;
    let mut decoder = ZlibDecoder::new(input);
    let mut out = Vec::new();
    decoder
        .read_to_end(&mut out)
        .map_err(|e| NyxError::Decode(format!("deflate decode: {e}")))?;
    Ok(out)
}

fn zstd_encode(input: &[u8]) -> NyxResult<Vec<u8>> {
    zstd::encode_all(input, 0).map_err(|e| NyxError::Decode(format!("zstd encode: {e}")))
}

fn zstd_decode(input: &[u8]) -> NyxResult<Vec<u8>> {
    zstd::decode_all(input).map_err(|e| NyxError::Decode(format!("zstd decode: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_base64() {
        let enc = encode(Codec::Base64, "hello world").unwrap();
        assert_eq!(decode(Codec::Base64, &enc).unwrap(), "hello world");
    }

    #[test]
    fn round_trips_url() {
        let enc = encode(Codec::Url, "hello world & more!").unwrap();
        assert!(enc.contains("%20"));
        assert_eq!(
            decode(Codec::Url, &enc).unwrap(),
            "hello world & more!"
        );
    }

    #[test]
    fn round_trips_html() {
        let enc = encode(Codec::Html, "<script>alert(1)</script>").unwrap();
        assert!(enc.contains("&lt;"));
        assert_eq!(
            decode(Codec::Html, &enc).unwrap(),
            "<script>alert(1)</script>"
        );
    }

    #[test]
    fn round_trips_hex() {
        let enc = encode(Codec::Hex, "AB").unwrap();
        assert_eq!(enc, "4142");
        assert_eq!(decode(Codec::Hex, "4142").unwrap(), "AB");
    }

    #[test]
    fn round_trips_gzip() {
        let enc = encode(Codec::Gzip, "compress me").unwrap();
        assert_eq!(decode(Codec::Gzip, &enc).unwrap(), "compress me");
    }

    #[test]
    fn round_trips_zstd() {
        let enc = encode(Codec::Zstd, "compress me with zstd").unwrap();
        assert_eq!(decode(Codec::Zstd, &enc).unwrap(), "compress me with zstd");
    }

    #[test]
    fn smart_decode_finds_base64() {
        let results = smart_decode("aGVsbG8gd29ybGQ=");
        assert!(results.iter().any(|r| r.codec == Codec::Base64 && r.output == "hello world"));
    }
}
