//! Wire types shared between the proxy core, the Tauri bridge, and the GUI.

use std::collections::HashMap;

use base64::Engine;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A single header pair preserved in capture order.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeaderEntry {
    pub name: String,
    pub value: String,
}

impl HeaderEntry {
    pub fn new(name: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            value: value.into(),
        }
    }
}

/// A captured HTTP request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapturedRequest {
    pub method: String,
    pub url: String,
    pub scheme: String,
    pub authority: String,
    pub path: String,
    pub http_version: String,
    pub headers: Vec<HeaderEntry>,
    /// Body bytes, encoded as base64 to survive the JSON bridge.
    pub body_b64: String,
    pub body_size: usize,
}

impl CapturedRequest {
    pub fn body_bytes(&self) -> Vec<u8> {
        base64::engine::general_purpose::STANDARD
            .decode(&self.body_b64)
            .unwrap_or_default()
    }

    pub fn body_as_string(&self) -> String {
        String::from_utf8_lossy(&self.body_bytes()).into_owned()
    }

    pub fn header_map(&self) -> HashMap<String, String> {
        self.headers
            .iter()
            .map(|h| (h.name.to_ascii_lowercase(), h.value.clone()))
            .collect()
    }
}

/// A captured HTTP response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapturedResponse {
    pub status: u16,
    pub http_version: String,
    pub reason: String,
    pub headers: Vec<HeaderEntry>,
    pub body_b64: String,
    pub body_size: usize,
    pub elapsed_ms: u64,
}

impl CapturedResponse {
    pub fn body_bytes(&self) -> Vec<u8> {
        base64::engine::general_purpose::STANDARD
            .decode(&self.body_b64)
            .unwrap_or_default()
    }

    pub fn body_as_string(&self) -> String {
        String::from_utf8_lossy(&self.body_bytes()).into_owned()
    }
}

/// A complete request/response pair.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpFlow {
    pub id: Uuid,
    pub started_at: DateTime<Utc>,
    pub request: CapturedRequest,
    pub response: Option<CapturedResponse>,
    pub tags: Vec<String>,
    pub error: Option<String>,
}

impl HttpFlow {
    pub fn new(request: CapturedRequest) -> Self {
        Self {
            id: Uuid::new_v4(),
            started_at: Utc::now(),
            request,
            response: None,
            tags: Vec::new(),
            error: None,
        }
    }

    pub fn host(&self) -> &str {
        self.request.authority.as_str()
    }
}

/// Events streamed from the proxy core to subscribers (Tauri events → React).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ProxyEvent {
    Started { listen_addr: String },
    Stopped,
    Flow { flow: HttpFlow },
    Log { level: String, message: String },
    Error { message: String },
}
