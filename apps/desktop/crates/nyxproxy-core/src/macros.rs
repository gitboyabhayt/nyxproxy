//! Macros — chains of HTTP requests played back in order, with variable
//! extraction between steps.
//!
//! Closes the same gap as Burp's "Session handling rules → macros". A typical
//! use case is a login flow: step 1 hits `/login` to set cookies and grab a
//! CSRF token from the response, step 2 uses `{{csrf_token}}` and the new
//! `Cookie` header against the protected endpoint.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use base64::Engine;
use chrono::{DateTime, Utc};
use parking_lot::RwLock;
use regex::Regex;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{NyxError, NyxResult};
use crate::model::{CapturedResponse, HeaderEntry};
use crate::repeater::{self, RepeaterRequest};

/// Where to look for an extraction value inside the response.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExtractionSource {
    /// Match a header by case-insensitive name. `pattern` is the header name.
    Header,
    /// Apply a JSON Pointer (RFC 6901) to the JSON-decoded response body.
    /// `pattern` is the pointer, e.g. `/data/token`.
    JsonPointer,
    /// First capture group of a regex against the (UTF-8-decoded) response
    /// body. `pattern` is the regex.
    BodyRegex,
    /// A specific cookie value from `Set-Cookie` response headers. `pattern`
    /// is the cookie name.
    Cookie,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Extraction {
    /// Variable name. Available as `{{name}}` in subsequent steps.
    pub name: String,
    pub source: ExtractionSource,
    pub pattern: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MacroStep {
    pub id: String,
    pub name: String,
    pub request: RepeaterRequest,
    #[serde(default)]
    pub extractions: Vec<Extraction>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Macro {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub steps: Vec<MacroStep>,
    #[serde(default = "Utc::now")]
    pub created_at: DateTime<Utc>,
    #[serde(default = "Utc::now")]
    pub updated_at: DateTime<Utc>,
}

impl Macro {
    pub fn new(name: impl Into<String>) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4().to_string(),
            name: name.into(),
            description: String::new(),
            steps: Vec::new(),
            created_at: now,
            updated_at: now,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MacroStepResult {
    pub step_id: String,
    pub step_name: String,
    pub request: RepeaterRequest,
    pub response: Option<CapturedResponse>,
    pub extracted: HashMap<String, String>,
    pub duration_ms: u64,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MacroRunResult {
    pub macro_id: String,
    pub macro_name: String,
    pub started_at: DateTime<Utc>,
    pub steps: Vec<MacroStepResult>,
    pub final_variables: HashMap<String, String>,
    pub succeeded: bool,
}

/// Replace every `{{var}}` token in `input` with the variable value if one
/// exists. Unknown placeholders are left untouched.
pub fn interpolate(input: &str, vars: &HashMap<String, String>) -> String {
    if !input.contains("{{") {
        return input.to_string();
    }
    let mut out = String::with_capacity(input.len());
    let bytes = input.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if i + 1 < bytes.len() && bytes[i] == b'{' && bytes[i + 1] == b'{' {
            if let Some(end) = input[i + 2..].find("}}") {
                let name = &input[i + 2..i + 2 + end];
                if let Some(value) = vars.get(name.trim()) {
                    out.push_str(value);
                    i = i + 2 + end + 2;
                    continue;
                }
            }
        }
        out.push(input.as_bytes()[i] as char);
        i += 1;
    }
    out
}

fn apply_extraction(
    extraction: &Extraction,
    response: &CapturedResponse,
) -> NyxResult<Option<String>> {
    match extraction.source {
        ExtractionSource::Header => {
            for h in &response.headers {
                if h.name.eq_ignore_ascii_case(&extraction.pattern) {
                    return Ok(Some(h.value.clone()));
                }
            }
            Ok(None)
        }
        ExtractionSource::Cookie => {
            for h in &response.headers {
                if !h.name.eq_ignore_ascii_case("set-cookie") {
                    continue;
                }
                // first segment is "name=value"
                let first = h.value.split(';').next().unwrap_or("");
                if let Some((name, value)) = first.split_once('=') {
                    if name.trim() == extraction.pattern {
                        return Ok(Some(value.trim().to_string()));
                    }
                }
            }
            Ok(None)
        }
        ExtractionSource::BodyRegex => {
            let body = base64::engine::general_purpose::STANDARD
                .decode(&response.body_b64)
                .unwrap_or_default();
            let text = String::from_utf8_lossy(&body);
            let re = Regex::new(&extraction.pattern)
                .map_err(|e| NyxError::Invalid(format!("invalid regex: {e}")))?;
            if let Some(caps) = re.captures(&text) {
                let value = caps.get(1).map(|m| m.as_str()).unwrap_or_else(|| {
                    caps.get(0).map(|m| m.as_str()).unwrap_or("")
                });
                return Ok(Some(value.to_string()));
            }
            Ok(None)
        }
        ExtractionSource::JsonPointer => {
            let body = base64::engine::general_purpose::STANDARD
                .decode(&response.body_b64)
                .unwrap_or_default();
            let value: serde_json::Value = match serde_json::from_slice(&body) {
                Ok(v) => v,
                Err(_) => return Ok(None),
            };
            if let Some(found) = value.pointer(&extraction.pattern) {
                let s = match found {
                    serde_json::Value::String(s) => s.clone(),
                    other => other.to_string(),
                };
                return Ok(Some(s));
            }
            Ok(None)
        }
    }
}

fn interpolate_request(
    request: &RepeaterRequest,
    vars: &HashMap<String, String>,
) -> RepeaterRequest {
    RepeaterRequest {
        method: interpolate(&request.method, vars),
        url: interpolate(&request.url, vars),
        headers: request
            .headers
            .iter()
            .map(|h| HeaderEntry {
                name: interpolate(&h.name, vars),
                value: interpolate(&h.value, vars),
            })
            .collect(),
        body_b64: {
            let body = base64::engine::general_purpose::STANDARD
                .decode(&request.body_b64)
                .unwrap_or_default();
            let text = String::from_utf8_lossy(&body);
            let next = interpolate(&text, vars);
            base64::engine::general_purpose::STANDARD.encode(next.as_bytes())
        },
        follow_redirects: request.follow_redirects,
        insecure: request.insecure,
    }
}

pub async fn run_macro(
    mac: &Macro,
    initial_vars: HashMap<String, String>,
) -> MacroRunResult {
    let started_at = Utc::now();
    let mut vars = initial_vars;
    let mut step_results = Vec::with_capacity(mac.steps.len());
    let mut all_ok = true;
    for step in &mac.steps {
        let request = interpolate_request(&step.request, &vars);
        let start = Instant::now();
        let response = repeater::send(&request).await;
        let duration_ms = start.elapsed().as_millis() as u64;
        match response {
            Ok(resp) => {
                let mut extracted = HashMap::new();
                let mut extract_error: Option<String> = None;
                for extraction in &step.extractions {
                    match apply_extraction(extraction, &resp) {
                        Ok(Some(value)) => {
                            extracted.insert(extraction.name.clone(), value.clone());
                            vars.insert(extraction.name.clone(), value);
                        }
                        Ok(None) => {
                            // soft miss — variable simply not populated
                        }
                        Err(err) => {
                            extract_error = Some(err.to_string());
                            all_ok = false;
                        }
                    }
                }
                step_results.push(MacroStepResult {
                    step_id: step.id.clone(),
                    step_name: step.name.clone(),
                    request,
                    response: Some(resp),
                    extracted,
                    duration_ms,
                    error: extract_error,
                });
            }
            Err(err) => {
                all_ok = false;
                step_results.push(MacroStepResult {
                    step_id: step.id.clone(),
                    step_name: step.name.clone(),
                    request,
                    response: None,
                    extracted: HashMap::new(),
                    duration_ms,
                    error: Some(err.to_string()),
                });
                break; // halt subsequent steps on transport error
            }
        }
    }
    MacroRunResult {
        macro_id: mac.id.clone(),
        macro_name: mac.name.clone(),
        started_at,
        steps: step_results,
        final_variables: vars,
        succeeded: all_ok,
    }
}

#[derive(Clone)]
pub struct MacroStore {
    path: PathBuf,
    inner: Arc<RwLock<HashMap<String, Macro>>>,
}

impl MacroStore {
    pub fn open(path: impl AsRef<Path>) -> NyxResult<Self> {
        let path = path.as_ref().to_path_buf();
        let mut map = HashMap::new();
        if path.exists() {
            let bytes = std::fs::read(&path).map_err(NyxError::Io)?;
            if !bytes.is_empty() {
                let loaded: Vec<Macro> = serde_json::from_slice(&bytes)
                    .map_err(|e| NyxError::Decode(format!("macros.json: {e}")))?;
                for mac in loaded {
                    map.insert(mac.id.clone(), mac);
                }
            }
        }
        Ok(Self {
            path,
            inner: Arc::new(RwLock::new(map)),
        })
    }

    pub fn list(&self) -> Vec<Macro> {
        let inner = self.inner.read();
        let mut v: Vec<Macro> = inner.values().cloned().collect();
        v.sort_by(|a, b| a.updated_at.cmp(&b.updated_at).reverse());
        v
    }

    pub fn get(&self, id: &str) -> Option<Macro> {
        self.inner.read().get(id).cloned()
    }

    pub fn save(&self, mut mac: Macro) -> NyxResult<Macro> {
        mac.updated_at = Utc::now();
        {
            let mut inner = self.inner.write();
            inner.insert(mac.id.clone(), mac.clone());
        }
        self.flush()?;
        Ok(mac)
    }

    pub fn delete(&self, id: &str) -> NyxResult<bool> {
        let removed = self.inner.write().remove(id).is_some();
        if removed {
            self.flush()?;
        }
        Ok(removed)
    }

    fn flush(&self) -> NyxResult<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent).map_err(NyxError::Io)?;
        }
        let snapshot: Vec<Macro> = self.inner.read().values().cloned().collect();
        let serialized =
            serde_json::to_vec_pretty(&snapshot).map_err(|e| NyxError::Decode(e.to_string()))?;
        std::fs::write(&self.path, serialized).map_err(NyxError::Io)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::HeaderEntry;

    fn resp(headers: Vec<(&str, &str)>, body: &str) -> CapturedResponse {
        CapturedResponse {
            status: 200,
            http_version: "HTTP/1.1".into(),
            reason: "OK".into(),
            headers: headers
                .into_iter()
                .map(|(n, v)| HeaderEntry {
                    name: n.into(),
                    value: v.into(),
                })
                .collect(),
            body_b64: base64::engine::general_purpose::STANDARD.encode(body.as_bytes()),
            body_size: body.len(),
            elapsed_ms: 0,
        }
    }

    #[test]
    fn interpolate_replaces_known_tokens() {
        let mut vars = HashMap::new();
        vars.insert("user".into(), "chandan".into());
        let out = interpolate("hello {{user}}, {{missing}} stays", &vars);
        assert_eq!(out, "hello chandan, {{missing}} stays");
    }

    #[test]
    fn extract_header_case_insensitive() {
        let r = resp(vec![("Content-Type", "application/json")], "");
        let e = Extraction {
            name: "ct".into(),
            source: ExtractionSource::Header,
            pattern: "content-type".into(),
        };
        let v = apply_extraction(&e, &r).unwrap();
        assert_eq!(v, Some("application/json".into()));
    }

    #[test]
    fn extract_cookie_by_name() {
        let r = resp(
            vec![
                ("Set-Cookie", "session=abc123; Path=/; HttpOnly"),
                ("Set-Cookie", "csrf=xyz; Path=/"),
            ],
            "",
        );
        let v = apply_extraction(
            &Extraction {
                name: "s".into(),
                source: ExtractionSource::Cookie,
                pattern: "session".into(),
            },
            &r,
        )
        .unwrap();
        assert_eq!(v, Some("abc123".into()));
    }

    #[test]
    fn extract_json_pointer() {
        let r = resp(
            vec![("content-type", "application/json")],
            r#"{"data":{"token":"tok-1"}}"#,
        );
        let v = apply_extraction(
            &Extraction {
                name: "tok".into(),
                source: ExtractionSource::JsonPointer,
                pattern: "/data/token".into(),
            },
            &r,
        )
        .unwrap();
        assert_eq!(v, Some("tok-1".into()));
    }

    #[test]
    fn extract_body_regex_first_group() {
        let r = resp(
            vec![("content-type", "text/html")],
            r#"<input name="csrf" value="abc123"/>"#,
        );
        let v = apply_extraction(
            &Extraction {
                name: "csrf".into(),
                source: ExtractionSource::BodyRegex,
                pattern: r#"name="csrf" value="([^"]+)""#.into(),
            },
            &r,
        )
        .unwrap();
        assert_eq!(v, Some("abc123".into()));
    }

    #[test]
    fn macro_store_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("macros.json");
        let store = MacroStore::open(&path).unwrap();
        let mac = Macro::new("Login");
        let saved = store.save(mac).unwrap();
        let loaded = MacroStore::open(&path).unwrap();
        let list = loaded.list();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, saved.id);
    }
}
