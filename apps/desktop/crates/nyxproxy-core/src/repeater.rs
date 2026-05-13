//! Repeater — re-issue a captured request after editing it.
//!
//! The repeater is intentionally not routed through the MITM proxy server: it
//! makes a direct HTTPS call out via `reqwest` so the user gets a clean,
//! noise-free reply pane to compare against the original capture.

use std::time::Instant;

use base64::Engine;
use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::error::{NyxError, NyxResult};
use crate::model::{CapturedRequest, CapturedResponse, HeaderEntry};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepeaterRequest {
    pub method: String,
    pub url: String,
    pub headers: Vec<HeaderEntry>,
    pub body_b64: String,
    /// When true, NyxProxy will follow 3xx redirects.
    #[serde(default)]
    pub follow_redirects: bool,
    /// When true, NyxProxy will skip TLS verification (e.g. for self-signed
    /// targets during testing).
    #[serde(default)]
    pub insecure: bool,
}

impl RepeaterRequest {
    pub fn from_captured(captured: &CapturedRequest) -> Self {
        Self {
            method: captured.method.clone(),
            url: captured.url.clone(),
            headers: captured.headers.clone(),
            body_b64: captured.body_b64.clone(),
            follow_redirects: false,
            insecure: false,
        }
    }
}

pub async fn send(req: &RepeaterRequest) -> NyxResult<CapturedResponse> {
    let mut builder = Client::builder()
        .user_agent("NyxProxy/0.1")
        .redirect(if req.follow_redirects {
            reqwest::redirect::Policy::limited(10)
        } else {
            reqwest::redirect::Policy::none()
        });
    if req.insecure {
        builder = builder.danger_accept_invalid_certs(true);
    }
    let client = builder
        .build()
        .map_err(|e| NyxError::Upstream(format!("build client: {e}")))?;

    let method = reqwest::Method::from_bytes(req.method.as_bytes())
        .map_err(|e| NyxError::BadRequest(format!("invalid method: {e}")))?;

    let mut http_req = client.request(method, &req.url);

    for header in &req.headers {
        // Skip hop-by-hop / forbidden headers — reqwest sets these.
        let lower = header.name.to_ascii_lowercase();
        if matches!(
            lower.as_str(),
            "host" | "content-length" | "connection" | "transfer-encoding"
        ) {
            continue;
        }
        http_req = http_req.header(header.name.clone(), header.value.clone());
    }

    let body_bytes = base64::engine::general_purpose::STANDARD
        .decode(req.body_b64.as_bytes())
        .map_err(|e| NyxError::BadRequest(format!("body base64: {e}")))?;
    if !body_bytes.is_empty() {
        http_req = http_req.body(body_bytes);
    }

    let start = Instant::now();
    let response = http_req
        .send()
        .await
        .map_err(|e| NyxError::Upstream(format!("send: {e}")))?;
    let status = response.status();
    let version = format!("{:?}", response.version());

    let mut headers: Vec<HeaderEntry> = Vec::with_capacity(response.headers().len());
    for (name, value) in response.headers().iter() {
        if let Ok(v) = value.to_str() {
            headers.push(HeaderEntry::new(name.as_str(), v));
        }
    }

    let body = response
        .bytes()
        .await
        .map_err(|e| NyxError::Upstream(format!("read body: {e}")))?;
    let elapsed_ms = start.elapsed().as_millis() as u64;

    Ok(CapturedResponse {
        status: status.as_u16(),
        http_version: version,
        reason: status.canonical_reason().unwrap_or("").to_string(),
        headers,
        body_size: body.len(),
        body_b64: base64::engine::general_purpose::STANDARD.encode(&body),
        elapsed_ms,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_captured_copies_fields() {
        let captured = CapturedRequest {
            method: "POST".into(),
            url: "https://example.com/api".into(),
            scheme: "https".into(),
            authority: "example.com".into(),
            path: "/api".into(),
            http_version: "HTTP/1.1".into(),
            headers: vec![HeaderEntry::new("Content-Type", "application/json")],
            body_b64: base64::engine::general_purpose::STANDARD.encode(b"{}"),
            body_size: 2,
        };
        let repeater = RepeaterRequest::from_captured(&captured);
        assert_eq!(repeater.method, "POST");
        assert_eq!(repeater.url, "https://example.com/api");
        assert_eq!(repeater.headers.len(), 1);
        assert!(!repeater.follow_redirects);
    }
}
