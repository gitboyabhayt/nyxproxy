//! JSON Web Token toolkit — decode, inspect, re-sign, brute-force HS256
//! secrets and surface common implementation weaknesses (alg=none accepted,
//! weak secrets, missing `exp`, RSA/HMAC confusion).
//!
//! This module is intentionally self-contained: HMAC-SHA-256 is implemented
//! inline so we don't need an extra dependency just for JWT. All public APIs
//! return `NyxResult`s so they slot into the existing command surface.
//!
//! Functions exposed:
//!
//! * [`decode`] — split-and-base64url-decode a token. Does not verify.
//! * [`encode_hs256`] / [`encode_none`] — re-sign a payload after editing.
//! * [`brute_hs256`] — try a list of candidate secrets against a token.
//! * [`analyze`] — high-level inspection that emits [`JwtFinding`]s.

use base64::Engine;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::error::{NyxError, NyxResult};

/// Decoded JWT view. `signature` is the raw base64url-encoded segment.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct JwtDecoded {
    pub header: serde_json::Value,
    pub payload: serde_json::Value,
    pub signature_b64: String,
    pub signing_input: String,
}

/// A potential weakness detected in a JWT.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct JwtFinding {
    pub kind: JwtFindingKind,
    pub severity: JwtSeverity,
    pub detail: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum JwtFindingKind {
    AlgNone,
    WeakAlgorithm,
    MissingExp,
    ExpiredToken,
    LongLivedToken,
    KidInjection,
    JkuJwkHeader,
    RsaHmacConfusion,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum JwtSeverity {
    Info,
    Low,
    Medium,
    High,
}

/// Output of a brute-force run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JwtBruteResult {
    pub tried: usize,
    pub secret: Option<String>,
    pub elapsed_ms: u128,
}

/// Decode a `header.payload.signature` token without verifying. Accepts both
/// padded and unpadded base64url. Returns an error for malformed input.
pub fn decode(token: &str) -> NyxResult<JwtDecoded> {
    let mut parts = token.split('.');
    let h_b64 = parts.next().ok_or_else(|| NyxError::Decode("missing header".into()))?;
    let p_b64 = parts.next().ok_or_else(|| NyxError::Decode("missing payload".into()))?;
    let s_b64 = parts.next().unwrap_or("");
    if parts.next().is_some() {
        return Err(NyxError::Decode("too many segments".into()));
    }

    let header_raw = decode_b64url(h_b64)?;
    let payload_raw = decode_b64url(p_b64)?;
    let header: serde_json::Value = serde_json::from_slice(&header_raw)
        .map_err(|e| NyxError::Decode(format!("header json: {e}")))?;
    let payload: serde_json::Value = serde_json::from_slice(&payload_raw)
        .map_err(|e| NyxError::Decode(format!("payload json: {e}")))?;

    Ok(JwtDecoded {
        header,
        payload,
        signature_b64: s_b64.to_string(),
        signing_input: format!("{}.{}", h_b64, p_b64),
    })
}

/// Re-sign a header + payload using HMAC-SHA-256. Returns the full token.
pub fn encode_hs256(
    header: &serde_json::Value,
    payload: &serde_json::Value,
    secret: &[u8],
) -> NyxResult<String> {
    // Force alg=HS256 in header so callers can't accidentally forge with
    // a mis-stated algorithm. We mutate a copy.
    let mut header = header.clone();
    if let Some(obj) = header.as_object_mut() {
        obj.insert("alg".into(), serde_json::Value::String("HS256".into()));
        obj.entry("typ").or_insert_with(|| serde_json::Value::String("JWT".into()));
    } else {
        return Err(NyxError::Decode("header must be a JSON object".into()));
    }

    let header_b64 = encode_b64url(serde_json::to_vec(&header).unwrap_or_default().as_slice());
    let payload_b64 = encode_b64url(serde_json::to_vec(payload).unwrap_or_default().as_slice());
    let signing_input = format!("{header_b64}.{payload_b64}");
    let sig = hmac_sha256(secret, signing_input.as_bytes());
    Ok(format!("{signing_input}.{}", encode_b64url(&sig)))
}

/// Re-sign with `alg: none` (signature segment empty). Used to demonstrate
/// implementations that accept unsigned tokens.
pub fn encode_none(
    header: &serde_json::Value,
    payload: &serde_json::Value,
) -> NyxResult<String> {
    let mut header = header.clone();
    if let Some(obj) = header.as_object_mut() {
        obj.insert("alg".into(), serde_json::Value::String("none".into()));
    } else {
        return Err(NyxError::Decode("header must be a JSON object".into()));
    }
    let header_b64 = encode_b64url(serde_json::to_vec(&header).unwrap_or_default().as_slice());
    let payload_b64 = encode_b64url(serde_json::to_vec(payload).unwrap_or_default().as_slice());
    Ok(format!("{header_b64}.{payload_b64}."))
}

/// Try a list of candidate secrets against a token's HMAC signature. Returns
/// the matching secret if found and the number of candidates attempted.
pub fn brute_hs256(token: &str, candidates: &[String]) -> NyxResult<JwtBruteResult> {
    let start = std::time::Instant::now();
    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() != 3 {
        return Err(NyxError::Decode("token must have 3 segments".into()));
    }
    let signing_input = format!("{}.{}", parts[0], parts[1]);
    let expected = decode_b64url(parts[2])?;

    for (i, cand) in candidates.iter().enumerate() {
        let mac = hmac_sha256(cand.as_bytes(), signing_input.as_bytes());
        if mac.as_slice() == expected.as_slice() {
            return Ok(JwtBruteResult {
                tried: i + 1,
                secret: Some(cand.clone()),
                elapsed_ms: start.elapsed().as_millis(),
            });
        }
    }
    Ok(JwtBruteResult {
        tried: candidates.len(),
        secret: None,
        elapsed_ms: start.elapsed().as_millis(),
    })
}

/// High-level analysis. Returns every weakness the toolkit knows how to spot.
pub fn analyze(token: &str) -> NyxResult<Vec<JwtFinding>> {
    let decoded = decode(token)?;
    let mut findings = Vec::new();
    let alg = decoded
        .header
        .get("alg")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_ascii_lowercase();

    if alg == "none" || alg == "" {
        findings.push(JwtFinding {
            kind: JwtFindingKind::AlgNone,
            severity: JwtSeverity::High,
            detail: "Token uses `alg: none` — server may accept unsigned tokens.".into(),
        });
    } else if alg == "hs256" {
        findings.push(JwtFinding {
            kind: JwtFindingKind::WeakAlgorithm,
            severity: JwtSeverity::Info,
            detail: "HS256 is symmetric: anyone with the secret can forge tokens.".into(),
        });
    }

    if decoded.header.get("kid").is_some() {
        findings.push(JwtFinding {
            kind: JwtFindingKind::KidInjection,
            severity: JwtSeverity::Low,
            detail: "Header carries a `kid`. Verify the server treats it as opaque and not as a file path / SQL identifier.".into(),
        });
    }
    if decoded.header.get("jku").is_some() || decoded.header.get("jwk").is_some() {
        findings.push(JwtFinding {
            kind: JwtFindingKind::JkuJwkHeader,
            severity: JwtSeverity::Medium,
            detail: "Header references an external key (`jku` / `jwk`). Verify the issuer pins to a known origin.".into(),
        });
    }

    let now = chrono::Utc::now().timestamp();
    let exp = decoded
        .payload
        .get("exp")
        .and_then(|v| v.as_i64().or_else(|| v.as_f64().map(|f| f as i64)));
    match exp {
        None => findings.push(JwtFinding {
            kind: JwtFindingKind::MissingExp,
            severity: JwtSeverity::Medium,
            detail: "Payload has no `exp` claim — token never expires.".into(),
        }),
        Some(t) if t < now => findings.push(JwtFinding {
            kind: JwtFindingKind::ExpiredToken,
            severity: JwtSeverity::Info,
            detail: format!("Token expired at unix={t}."),
        }),
        Some(t) if (t - now) > 60 * 60 * 24 * 30 => findings.push(JwtFinding {
            kind: JwtFindingKind::LongLivedToken,
            severity: JwtSeverity::Low,
            detail: format!("Token lifetime > 30 days (exp={t}). Consider shorter."),
        }),
        _ => {}
    }

    // RSA/HMAC confusion hint: if alg is HS* and there is a known RSA public
    // key in the payload (some legacy implementations store one) flag it.
    if alg.starts_with("hs")
        && decoded
            .payload
            .as_object()
            .map(|m| m.contains_key("public_key") || m.contains_key("rsa_pub"))
            .unwrap_or(false)
    {
        findings.push(JwtFinding {
            kind: JwtFindingKind::RsaHmacConfusion,
            severity: JwtSeverity::High,
            detail: "Payload exposes a public key while the token is HMAC-signed — classic RS→HS confusion attack surface.".into(),
        });
    }

    Ok(findings)
}

// --- helpers ---------------------------------------------------------------

fn decode_b64url(s: &str) -> NyxResult<Vec<u8>> {
    let cleaned = s.trim().trim_end_matches('=');
    base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(cleaned.as_bytes())
        .map_err(|e| NyxError::Decode(format!("base64url: {e}")))
}

fn encode_b64url(bytes: &[u8]) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

/// Pure-Rust HMAC-SHA-256 so we don't pull in the `hmac` crate.
fn hmac_sha256(key: &[u8], message: &[u8]) -> Vec<u8> {
    const BLOCK: usize = 64;
    let mut k = if key.len() > BLOCK {
        let mut h = Sha256::new();
        h.update(key);
        h.finalize().to_vec()
    } else {
        key.to_vec()
    };
    k.resize(BLOCK, 0);

    let mut ipad = [0u8; BLOCK];
    let mut opad = [0u8; BLOCK];
    for i in 0..BLOCK {
        ipad[i] = k[i] ^ 0x36;
        opad[i] = k[i] ^ 0x5c;
    }

    let mut inner = Sha256::new();
    inner.update(ipad);
    inner.update(message);
    let inner_digest = inner.finalize();

    let mut outer = Sha256::new();
    outer.update(opad);
    outer.update(inner_digest);
    outer.finalize().to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;

    // Standard JWT test vector from RFC 7519: HS256 with secret "your-256-bit-secret"
    const TOKEN: &str = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.\
                         eyJzdWIiOiIxMjM0NTY3ODkwIiwibmFtZSI6IkpvaG4gRG9lIiwiaWF0IjoxNTE2MjM5MDIyfQ.\
                         SflKxwRJSMeKKF2QT4fwpMeJf36POk6yJV_adQssw5c";

    #[test]
    fn decodes_standard_token() {
        let d = decode(TOKEN).expect("decode");
        assert_eq!(d.header.get("alg").and_then(|v| v.as_str()), Some("HS256"));
        assert_eq!(d.payload.get("sub").and_then(|v| v.as_str()), Some("1234567890"));
    }

    #[test]
    fn brute_finds_known_secret() {
        let r = brute_hs256(
            TOKEN,
            &[
                "wrong".into(),
                "also-wrong".into(),
                "your-256-bit-secret".into(),
                "never-tried".into(),
            ],
        )
        .expect("brute");
        assert_eq!(r.secret.as_deref(), Some("your-256-bit-secret"));
        assert_eq!(r.tried, 3);
    }

    #[test]
    fn brute_returns_none_when_no_match() {
        let r = brute_hs256(TOKEN, &["alpha".into(), "beta".into()]).expect("brute");
        assert!(r.secret.is_none());
        assert_eq!(r.tried, 2);
    }

    #[test]
    fn round_trip_hs256_verifies_with_known_secret() {
        // Note: we don't assert byte-equality against the original token
        // because serde_json sorts object keys alphabetically while the
        // original was written in a different order. Instead we re-encode
        // and verify the re-encoded token brute-force-checks against the
        // same secret.
        let d = decode(TOKEN).expect("decode");
        let re = encode_hs256(&d.header, &d.payload, b"your-256-bit-secret").expect("encode");
        let again = decode(&re).expect("decode re-encoded");
        assert_eq!(again.payload, d.payload);
        let r =
            brute_hs256(&re, &["nope".into(), "your-256-bit-secret".into()]).expect("brute");
        assert_eq!(r.secret.as_deref(), Some("your-256-bit-secret"));
    }

    #[test]
    fn alg_none_produces_three_segments_last_empty() {
        let d = decode(TOKEN).expect("decode");
        let none = encode_none(&d.header, &d.payload).expect("encode none");
        let parts: Vec<&str> = none.split('.').collect();
        assert_eq!(parts.len(), 3);
        assert!(parts[2].is_empty());
    }

    #[test]
    fn analyze_flags_alg_none() {
        let d = decode(TOKEN).expect("decode");
        let unsigned = encode_none(&d.header, &d.payload).expect("encode none");
        let findings = analyze(&unsigned).expect("analyze");
        assert!(findings.iter().any(|f| matches!(f.kind, JwtFindingKind::AlgNone)));
    }

    #[test]
    fn analyze_flags_missing_exp() {
        let findings = analyze(TOKEN).expect("analyze");
        assert!(findings.iter().any(|f| matches!(f.kind, JwtFindingKind::MissingExp)));
    }

    #[test]
    fn decode_rejects_malformed() {
        assert!(decode("not.a.token.extra").is_err());
        assert!(decode("only-one-segment").is_err());
    }
}
