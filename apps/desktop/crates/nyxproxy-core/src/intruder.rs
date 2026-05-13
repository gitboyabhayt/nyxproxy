//! Intruder — automated request fuzzer.
//!
//! Phase 1 implements **Sniper** mode: a single insertion point per request,
//! replacing a marker token (``§``) with each payload in turn. The result is
//! a streaming list of completed attempts that the UI can render live.

use std::time::Instant;

use base64::Engine;
use futures_util::stream::{self, Stream, StreamExt};
use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::error::{NyxError, NyxResult};
use crate::model::{CapturedRequest, HeaderEntry};

pub const MARKER: &str = "§";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttackType {
    Sniper,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntruderConfig {
    pub template: CapturedRequest,
    pub payloads: Vec<String>,
    pub attack: AttackType,
    /// Maximum concurrency (default 8).
    #[serde(default = "default_concurrency")]
    pub concurrency: usize,
    /// When true, skip TLS verification.
    #[serde(default)]
    pub insecure: bool,
}

fn default_concurrency() -> usize {
    8
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntruderAttempt {
    pub index: usize,
    pub payload: String,
    pub status: Option<u16>,
    pub response_length: Option<usize>,
    pub elapsed_ms: u64,
    pub error: Option<String>,
    pub snippet: Option<String>,
}

/// Replace every occurrence of the marker token in a string. Sniper applies
/// the same payload to all marker occurrences in the request.
fn apply_payload(template: &CapturedRequest, payload: &str) -> NyxResult<(String, String, Vec<HeaderEntry>, Vec<u8>)> {
    let body_bytes = base64::engine::general_purpose::STANDARD
        .decode(template.body_b64.as_bytes())
        .map_err(|e| NyxError::BadRequest(format!("body base64: {e}")))?;
    let body_str = String::from_utf8_lossy(&body_bytes).into_owned();

    let url = template.url.replace(MARKER, payload);
    let method = template.method.clone();

    let headers: Vec<HeaderEntry> = template
        .headers
        .iter()
        .map(|h| HeaderEntry::new(h.name.replace(MARKER, payload), h.value.replace(MARKER, payload)))
        .collect();
    let body_replaced = body_str.replace(MARKER, payload);
    Ok((method, url, headers, body_replaced.into_bytes()))
}

pub fn run<'a>(
    cfg: &'a IntruderConfig,
) -> impl Stream<Item = IntruderAttempt> + 'a {
    let client_builder = Client::builder()
        .user_agent("NyxProxy/0.1")
        .redirect(reqwest::redirect::Policy::none());
    let client_builder = if cfg.insecure {
        client_builder.danger_accept_invalid_certs(true)
    } else {
        client_builder
    };
    let client = client_builder.build().expect("build reqwest client");

    let concurrency = cfg.concurrency.max(1);
    stream::iter(cfg.payloads.iter().enumerate())
        .map(move |(index, payload)| {
            let client = client.clone();
            let template = cfg.template.clone();
            let payload = payload.clone();
            async move {
                let start = Instant::now();
                match execute_single(&client, &template, &payload).await {
                    Ok((status, len, snippet)) => IntruderAttempt {
                        index,
                        payload,
                        status: Some(status),
                        response_length: Some(len),
                        elapsed_ms: start.elapsed().as_millis() as u64,
                        error: None,
                        snippet: Some(snippet),
                    },
                    Err(err) => IntruderAttempt {
                        index,
                        payload,
                        status: None,
                        response_length: None,
                        elapsed_ms: start.elapsed().as_millis() as u64,
                        error: Some(err.to_string()),
                        snippet: None,
                    },
                }
            }
        })
        .buffer_unordered(concurrency)
}

async fn execute_single(
    client: &Client,
    template: &CapturedRequest,
    payload: &str,
) -> NyxResult<(u16, usize, String)> {
    let (method, url, headers, body) = apply_payload(template, payload)?;
    let method = reqwest::Method::from_bytes(method.as_bytes())
        .map_err(|e| NyxError::BadRequest(format!("invalid method: {e}")))?;
    let mut req = client.request(method, &url);
    for header in headers {
        let lower = header.name.to_ascii_lowercase();
        if matches!(
            lower.as_str(),
            "host" | "content-length" | "connection" | "transfer-encoding"
        ) {
            continue;
        }
        req = req.header(header.name, header.value);
    }
    if !body.is_empty() {
        req = req.body(body);
    }
    let response = req
        .send()
        .await
        .map_err(|e| NyxError::Upstream(format!("send: {e}")))?;
    let status = response.status().as_u16();
    let bytes = response
        .bytes()
        .await
        .map_err(|e| NyxError::Upstream(format!("read body: {e}")))?;
    let len = bytes.len();
    let snippet = String::from_utf8_lossy(&bytes)
        .chars()
        .take(256)
        .collect::<String>();
    Ok((status, len, snippet))
}

/// Run the entire attack and collect every result. Mainly used in tests; the
/// UI consumes the stream from [`run`] for live updates.
pub async fn run_to_completion(cfg: &IntruderConfig) -> Vec<IntruderAttempt> {
    run(cfg).collect::<Vec<_>>().await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn template() -> CapturedRequest {
        CapturedRequest {
            method: "GET".into(),
            url: format!("https://example.invalid/search?q={MARKER}"),
            scheme: "https".into(),
            authority: "example.invalid".into(),
            path: "/search".into(),
            http_version: "HTTP/1.1".into(),
            headers: vec![HeaderEntry::new("Accept", "text/html")],
            body_b64: base64::engine::general_purpose::STANDARD.encode(b""),
            body_size: 0,
        }
    }

    #[test]
    fn apply_payload_substitutes_marker() {
        let (method, url, headers, body) = apply_payload(&template(), "abc").unwrap();
        assert_eq!(method, "GET");
        assert!(url.ends_with("q=abc"));
        assert_eq!(headers.len(), 1);
        assert!(body.is_empty());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn run_emits_one_attempt_per_payload() {
        let cfg = IntruderConfig {
            template: template(),
            payloads: vec!["a".into(), "b".into(), "c".into()],
            attack: AttackType::Sniper,
            concurrency: 4,
            insecure: false,
        };
        let attempts = run_to_completion(&cfg).await;
        assert_eq!(attempts.len(), 3);
        // Every attempt should resolve (with an error or status), not get lost.
        let payloads: Vec<_> = attempts.iter().map(|a| a.payload.clone()).collect();
        for p in ["a", "b", "c"] {
            assert!(payloads.contains(&p.to_string()), "missing payload {p}");
        }
    }
}
