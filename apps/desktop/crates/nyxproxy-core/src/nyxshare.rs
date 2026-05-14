//! Encrypted evidence packs — `.nyxshare` (Leapfrog #8).
//!
//! A `.nyxshare` is a sealed, password-protected bundle that contains
//! the request/response flows, scope, and issues needed to fully
//! reproduce a finding on another machine. Unlike `.nyxproxy`
//! workspaces (plaintext zstd-JSON), shares are designed to be sent
//! through untrusted channels: email, JIRA, chat. The recipient must
//! know the password to open it.
//!
//! ## On-disk layout
//!
//! ```text
//!   magic      : 8 bytes  = b"NYXSHARE"
//!   version    : 1 byte   = 0x01
//!   reserved   : 3 bytes  = 0x00 0x00 0x00
//!   argon2_salt: 16 bytes
//!   chacha_nonce: 12 bytes
//!   payload    : ciphertext + 16-byte poly1305 tag
//! ```
//!
//! Plaintext is zstd-compressed JSON: `{"manifest": ..., "flows": [...], "issues": [...]}`.
//!
//! Key derivation: Argon2id (memory=19 MiB, time=2, parallelism=1).
//! Cipher: ChaCha20-Poly1305. Both AEADs and AKD parameters are
//! conservative defaults intended to make brute-forcing on commodity
//! hardware impractical while remaining usable on a laptop.

use crate::error::{NyxError, NyxResult};
use crate::model::HttpFlow;
use crate::scanner::Issue;
use argon2::{Algorithm, Argon2, Params, Version};
use chacha20poly1305::aead::{Aead, KeyInit, OsRng};
use chacha20poly1305::{ChaCha20Poly1305, Key, Nonce};
use rand::RngCore;
use serde::{Deserialize, Serialize};

const MAGIC: &[u8; 8] = b"NYXSHARE";
const VERSION: u8 = 0x01;
const SALT_LEN: usize = 16;
const NONCE_LEN: usize = 12;
const HEADER_LEN: usize = 8 + 1 + 3 + SALT_LEN + NONCE_LEN;

/// Metadata stored in plaintext within the encrypted payload (but
/// shown to operators after they decrypt). The user can include a
/// note explaining what the share is for.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShareManifest {
    pub created_at: String,
    pub app_version: String,
    pub note: String,
    pub flow_count: usize,
    pub issue_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SharePayload {
    pub manifest: ShareManifest,
    pub flows: Vec<HttpFlow>,
    pub issues: Vec<Issue>,
}

pub fn seal(payload: &SharePayload, password: &str) -> NyxResult<Vec<u8>> {
    if password.is_empty() {
        return Err(NyxError::BadRequest(
            "password cannot be empty".into(),
        ));
    }
    let json = serde_json::to_vec(payload)
        .map_err(|e| NyxError::Internal(format!("serialize: {e}")))?;
    let compressed = zstd::stream::encode_all(&json[..], 19)
        .map_err(|e| NyxError::Internal(format!("compress: {e}")))?;

    let mut salt = [0u8; SALT_LEN];
    let mut nonce = [0u8; NONCE_LEN];
    OsRng.fill_bytes(&mut salt);
    OsRng.fill_bytes(&mut nonce);

    let key = derive_key(password, &salt)?;
    let cipher = ChaCha20Poly1305::new(Key::from_slice(&key));
    let ciphertext = cipher
        .encrypt(Nonce::from_slice(&nonce), compressed.as_ref())
        .map_err(|e| NyxError::Internal(format!("encrypt: {e}")))?;

    let mut out = Vec::with_capacity(HEADER_LEN + ciphertext.len());
    out.extend_from_slice(MAGIC);
    out.push(VERSION);
    out.extend_from_slice(&[0u8; 3]);
    out.extend_from_slice(&salt);
    out.extend_from_slice(&nonce);
    out.extend_from_slice(&ciphertext);
    Ok(out)
}

pub fn unseal(bytes: &[u8], password: &str) -> NyxResult<SharePayload> {
    if bytes.len() < HEADER_LEN {
        return Err(NyxError::BadRequest(
            "share is truncated (no header)".into(),
        ));
    }
    if &bytes[..8] != MAGIC {
        return Err(NyxError::BadRequest(
            "not a .nyxshare bundle (bad magic)".into(),
        ));
    }
    if bytes[8] != VERSION {
        return Err(NyxError::BadRequest(format!(
            "unsupported share version: {}",
            bytes[8]
        )));
    }
    let salt: &[u8; SALT_LEN] = bytes[12..12 + SALT_LEN]
        .try_into()
        .expect("slice size");
    let nonce: &[u8; NONCE_LEN] = bytes[12 + SALT_LEN..HEADER_LEN]
        .try_into()
        .expect("slice size");
    let ciphertext = &bytes[HEADER_LEN..];

    let key = derive_key(password, salt)?;
    let cipher = ChaCha20Poly1305::new(Key::from_slice(&key));
    let compressed = cipher
        .decrypt(Nonce::from_slice(nonce), ciphertext)
        .map_err(|_| NyxError::BadRequest("decryption failed (wrong password or corrupted)".into()))?;

    let json = zstd::stream::decode_all(&compressed[..])
        .map_err(|e| NyxError::Internal(format!("decompress: {e}")))?;
    serde_json::from_slice(&json)
        .map_err(|e| NyxError::Internal(format!("parse json: {e}")))
}

fn derive_key(password: &str, salt: &[u8; SALT_LEN]) -> NyxResult<[u8; 32]> {
    let params = Params::new(19 * 1024, 2, 1, Some(32))
        .map_err(|e| NyxError::Internal(format!("argon2 params: {e}")))?;
    let argon = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    let mut key = [0u8; 32];
    argon
        .hash_password_into(password.as_bytes(), salt, &mut key)
        .map_err(|e| NyxError::Internal(format!("argon2 derive: {e}")))?;
    Ok(key)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{CapturedRequest, HttpFlow};
    use chrono::Utc;
    use std::collections::BTreeMap;
    use uuid::Uuid;

    fn sample_payload() -> SharePayload {
        let flow = HttpFlow {
            id: Uuid::new_v4(),
            started_at: Utc::now(),
            request: CapturedRequest {
                method: "GET".to_string(),
                url: "https://example.com/".to_string(),
                scheme: "https".to_string(),
                authority: "example.com".to_string(),
                path: "/".to_string(),
                http_version: "HTTP/1.1".to_string(),
                headers: vec![],
                body_b64: String::new(),
                body_size: 0,
            },
            response: None,
            tags: vec!["sample".to_string()],
            error: None,
        };
        let _ignored: BTreeMap<&str, &str> = BTreeMap::new();
        SharePayload {
            manifest: ShareManifest {
                created_at: "2026-05-14T00:00:00Z".to_string(),
                app_version: "test".to_string(),
                note: "demo share".to_string(),
                flow_count: 1,
                issue_count: 0,
            },
            flows: vec![flow],
            issues: vec![],
        }
    }

    #[test]
    fn round_trip_preserves_payload() {
        let p = sample_payload();
        let sealed = seal(&p, "correct horse battery staple").unwrap();
        let unsealed = unseal(&sealed, "correct horse battery staple").unwrap();
        assert_eq!(unsealed.manifest.note, p.manifest.note);
        assert_eq!(unsealed.flows.len(), 1);
        assert_eq!(unsealed.flows[0].request.url, p.flows[0].request.url);
    }

    #[test]
    fn wrong_password_fails() {
        let p = sample_payload();
        let sealed = seal(&p, "correct horse battery staple").unwrap();
        let err = unseal(&sealed, "wrong password").unwrap_err();
        assert!(err.to_string().to_lowercase().contains("decryption failed"));
    }

    #[test]
    fn rejects_short_input() {
        let err = unseal(b"too short", "anything").unwrap_err();
        assert!(err.to_string().contains("truncated"));
    }

    #[test]
    fn rejects_bad_magic() {
        let mut sealed = seal(&sample_payload(), "pw").unwrap();
        sealed[0] = b'X';
        let err = unseal(&sealed, "pw").unwrap_err();
        assert!(err.to_string().contains("bad magic"));
    }

    #[test]
    fn empty_password_rejected() {
        let err = seal(&sample_payload(), "").unwrap_err();
        assert!(err.to_string().contains("empty"));
    }

    #[test]
    fn header_layout_is_correct() {
        let sealed = seal(&sample_payload(), "pw").unwrap();
        assert_eq!(&sealed[..8], MAGIC);
        assert_eq!(sealed[8], VERSION);
        assert_eq!(&sealed[9..12], &[0u8, 0, 0]);
        // Salt and nonce are random; just check they fit.
        assert!(sealed.len() > HEADER_LEN + 16); // header + tag at minimum
    }
}
