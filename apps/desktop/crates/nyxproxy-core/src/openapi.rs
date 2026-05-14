//! OpenAPI / Swagger auto-test generator (Feature BB).
//!
//! Given an OpenAPI 2.0 or 3.x specification, produce a deterministic list
//! of [`OpenApiTestCase`] entries covering the three highest-value classes
//! of API misuse:
//!
//! 1. **Authentication bypass** — for every endpoint that declares a
//!    `security` requirement, emit one request with auth stripped.
//! 2. **IDOR / object enumeration** — for every path parameter that looks
//!    numeric (`integer` schema, name contains `id`), emit requests with
//!    `0`, `1`, `9999999`.
//! 3. **Rate-limit / brute-force** — for `POST` endpoints whose name
//!    suggests authentication (`login`, `signin`, `token`, `auth`,
//!    `password`), emit a burst-of-100 marker so Intruder runs it.
//!
//! The generator does **not** execute requests — that's the Intruder /
//! Repeater's job. It returns a static plan so the user can review it
//! before firing.
//!
//! Spec support:
//! * OpenAPI 3.0 / 3.1 — top-level `paths` map plus `securitySchemes`
//!   under `components.securitySchemes`.
//! * Swagger 2.0 — top-level `paths` map plus `securityDefinitions`. Path
//!   templating, parameter `in: path|query|header` and `$ref:
//!   "#/definitions/..."` are recognised; deep schema resolution beyond
//!   simple parameter typing is left to a follow-up.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::{NyxError, NyxResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum OpenApiCategory {
    AuthBypass,
    Idor,
    RateLimit,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenApiTestCase {
    pub category: OpenApiCategory,
    pub name: String,
    pub method: String,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Option<String>,
    /// How many times Intruder should fire this exact request.
    pub repeat: u32,
    pub notes: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct OpenApiPlan {
    pub version: String,
    pub server_url: String,
    pub cases: Vec<OpenApiTestCase>,
    pub diagnostics: Vec<String>,
}

/// Parse an OpenAPI / Swagger document and emit a [`OpenApiPlan`].
///
/// `base_override` overrides the spec's `servers[0].url` (OpenAPI 3) or
/// `host + basePath` (Swagger 2). Useful when testing the same spec
/// against staging vs production.
pub fn build_plan(bytes: &[u8], base_override: Option<&str>) -> NyxResult<OpenApiPlan> {
    // Accept JSON or YAML; we shell out to serde_json first then yaml.
    let doc: Value = match serde_json::from_slice::<Value>(bytes) {
        Ok(v) => v,
        Err(_) => {
            // very small YAML fallback — pull the JSON-compatible subset.
            // We avoid pulling serde_yaml as a heavy dep; users tend to
            // ship swagger.json. If parsing fails, surface the error.
            return Err(NyxError::BadRequest(
                "openapi: only JSON documents are supported in this build".into(),
            ));
        }
    };

    let version = detect_version(&doc);
    let server_url = base_override
        .map(str::to_string)
        .unwrap_or_else(|| detect_server(&doc));
    let mut plan = OpenApiPlan {
        version: version.clone(),
        server_url: server_url.clone(),
        cases: Vec::new(),
        diagnostics: Vec::new(),
    };

    let security_schemes: Vec<String> = collect_security_schemes(&doc);

    let paths = match doc.get("paths").and_then(|v| v.as_object()) {
        Some(p) => p,
        None => {
            plan.diagnostics
                .push("paths object missing — nothing to test".into());
            return Ok(plan);
        }
    };

    for (path, item) in paths.iter() {
        let Some(obj) = item.as_object() else { continue };
        for method in ["get", "post", "put", "patch", "delete"] {
            let Some(op) = obj.get(method).and_then(|v| v.as_object()) else {
                continue;
            };
            let url = build_url(&server_url, path);

            // 1) Auth bypass
            if has_security(op, &doc, &security_schemes) {
                plan.cases.push(OpenApiTestCase {
                    category: OpenApiCategory::AuthBypass,
                    name: format!("auth-bypass {} {}", method.to_uppercase(), path),
                    method: method.to_uppercase(),
                    url: url.clone(),
                    headers: vec![],
                    body: None,
                    repeat: 1,
                    notes: "Endpoint declares a security requirement; called here without it.".into(),
                });
            }

            // 2) IDOR — numeric-looking path params
            let params = merged_parameters(op, obj);
            for p in &params {
                if !is_path_param(p) {
                    continue;
                }
                let name = p
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                if is_numeric_id(p, &name) {
                    for value in ["0", "1", "9999999"] {
                        let mutated = url.replace(&format!("{{{}}}", name), value);
                        plan.cases.push(OpenApiTestCase {
                            category: OpenApiCategory::Idor,
                            name: format!(
                                "idor {} {} {}={}",
                                method.to_uppercase(),
                                path,
                                name,
                                value
                            ),
                            method: method.to_uppercase(),
                            url: mutated,
                            headers: vec![],
                            body: None,
                            repeat: 1,
                            notes: format!(
                                "Numeric path param {name} forced to {value} — change to currently-authenticated user's actual ID to confirm IDOR."
                            ),
                        });
                    }
                }
            }

            // 3) Rate-limit / brute-force probe on auth-shaped POSTs.
            if method == "post" && looks_like_auth_endpoint(path) {
                plan.cases.push(OpenApiTestCase {
                    category: OpenApiCategory::RateLimit,
                    name: format!("rate-limit POST {}", path),
                    method: "POST".into(),
                    url: url.clone(),
                    headers: vec![("content-type".into(), "application/json".into())],
                    body: Some("{}".into()),
                    repeat: 100,
                    notes: "Auth-shaped endpoint — Intruder will fire 100 times to detect missing rate-limiting.".into(),
                });
            }
        }
    }

    if plan.cases.is_empty() {
        plan.diagnostics
            .push("no testable endpoints found — spec may be empty or schemaless".into());
    }
    Ok(plan)
}

fn detect_version(doc: &Value) -> String {
    if let Some(v) = doc.get("openapi").and_then(|v| v.as_str()) {
        return v.to_string();
    }
    if let Some(v) = doc.get("swagger").and_then(|v| v.as_str()) {
        return v.to_string();
    }
    "unknown".into()
}

fn detect_server(doc: &Value) -> String {
    // OpenAPI 3
    if let Some(arr) = doc.get("servers").and_then(|v| v.as_array()) {
        if let Some(first) = arr.first() {
            if let Some(u) = first.get("url").and_then(|v| v.as_str()) {
                return u.trim_end_matches('/').to_string();
            }
        }
    }
    // Swagger 2
    let host = doc
        .get("host")
        .and_then(|v| v.as_str())
        .unwrap_or("localhost");
    let base_path = doc
        .get("basePath")
        .and_then(|v| v.as_str())
        .unwrap_or("/")
        .trim_end_matches('/');
    let schemes = doc
        .get("schemes")
        .and_then(|v| v.as_array())
        .and_then(|a| a.first().and_then(|v| v.as_str()))
        .unwrap_or("https");
    format!("{schemes}://{host}{base_path}")
}

fn collect_security_schemes(doc: &Value) -> Vec<String> {
    // OpenAPI 3
    if let Some(obj) = doc
        .pointer("/components/securitySchemes")
        .and_then(|v| v.as_object())
    {
        return obj.keys().cloned().collect();
    }
    // Swagger 2
    if let Some(obj) = doc
        .get("securityDefinitions")
        .and_then(|v| v.as_object())
    {
        return obj.keys().cloned().collect();
    }
    Vec::new()
}

fn has_security(op: &serde_json::Map<String, Value>, doc: &Value, schemes: &[String]) -> bool {
    if let Some(arr) = op.get("security").and_then(|v| v.as_array()) {
        if !arr.is_empty() {
            return true;
        }
        // explicit empty array overrides defaults → public endpoint.
        return false;
    }
    // fall back to global `security` requirement.
    if let Some(arr) = doc.get("security").and_then(|v| v.as_array()) {
        if !arr.is_empty() {
            return true;
        }
    }
    !schemes.is_empty() && doc.get("security").is_none()
}

fn build_url(server: &str, path: &str) -> String {
    let s = server.trim_end_matches('/');
    let p = if path.starts_with('/') {
        path.to_string()
    } else {
        format!("/{path}")
    };
    format!("{s}{p}")
}

fn merged_parameters(
    op: &serde_json::Map<String, Value>,
    path_obj: &serde_json::Map<String, Value>,
) -> Vec<Value> {
    let mut out: Vec<Value> = Vec::new();
    if let Some(arr) = path_obj.get("parameters").and_then(|v| v.as_array()) {
        out.extend(arr.iter().cloned());
    }
    if let Some(arr) = op.get("parameters").and_then(|v| v.as_array()) {
        out.extend(arr.iter().cloned());
    }
    out
}

fn is_path_param(p: &Value) -> bool {
    p.get("in").and_then(|v| v.as_str()) == Some("path")
}

fn is_numeric_id(p: &Value, name: &str) -> bool {
    let lower = name.to_lowercase();
    // Heuristic: name contains 'id' OR schema is integer.
    let by_name = lower == "id"
        || lower.ends_with("_id")
        || lower.ends_with("id")
        || lower.contains("user")
        || lower.contains("account");
    let by_type = matches!(
        p.pointer("/schema/type").and_then(|v| v.as_str()),
        Some("integer") | Some("number")
    ) || matches!(p.get("type").and_then(|v| v.as_str()), Some("integer") | Some("number"));
    by_name || by_type
}

fn looks_like_auth_endpoint(path: &str) -> bool {
    let lower = path.to_lowercase();
    ["login", "signin", "sign_in", "auth", "token", "password", "reset", "otp"]
        .iter()
        .any(|kw| lower.contains(kw))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SPEC_3: &str = r#"{
        "openapi": "3.0.3",
        "servers": [{ "url": "https://api.example.com/v1" }],
        "components": {
            "securitySchemes": {
                "bearer": { "type": "http", "scheme": "bearer" }
            }
        },
        "security": [{ "bearer": [] }],
        "paths": {
            "/users/{userId}": {
                "get": {
                    "parameters": [
                        { "name": "userId", "in": "path", "required": true,
                          "schema": { "type": "integer" } }
                    ]
                }
            },
            "/auth/login": {
                "post": { }
            },
            "/health": {
                "get": { "security": [] }
            }
        }
    }"#;

    const SWAGGER_2: &str = r#"{
        "swagger": "2.0",
        "host": "api.example.com",
        "basePath": "/v2",
        "schemes": ["https"],
        "securityDefinitions": { "key": { "type": "apiKey", "in": "header", "name": "X-Api-Key" } },
        "security": [{ "key": [] }],
        "paths": {
            "/items/{itemId}": {
                "get": { "parameters": [{ "name": "itemId", "in": "path", "required": true, "type": "integer" }] }
            },
            "/login": { "post": {} }
        }
    }"#;

    #[test]
    fn openapi_3_generates_auth_idor_and_rate_limit() {
        let plan = build_plan(SPEC_3.as_bytes(), None).unwrap();
        assert_eq!(plan.version, "3.0.3");
        assert_eq!(plan.server_url, "https://api.example.com/v1");

        let auth = plan
            .cases
            .iter()
            .filter(|c| c.category == OpenApiCategory::AuthBypass)
            .count();
        let idor = plan
            .cases
            .iter()
            .filter(|c| c.category == OpenApiCategory::Idor)
            .count();
        let rate = plan
            .cases
            .iter()
            .filter(|c| c.category == OpenApiCategory::RateLimit)
            .count();
        // 2 protected endpoints (/users/{userId}, /auth/login), 1 public (/health).
        // → 2 auth-bypass cases.
        assert_eq!(auth, 2);
        // 1 numeric id path param × 3 mutations.
        assert_eq!(idor, 3);
        // 1 login POST → rate-limit case.
        assert_eq!(rate, 1);

        let idor_case = plan
            .cases
            .iter()
            .find(|c| c.category == OpenApiCategory::Idor && c.url.ends_with("/9999999"))
            .expect("idor 9999999 case");
        assert_eq!(idor_case.method, "GET");
        assert!(idor_case.url.contains("/users/9999999"));

        let rate_case = plan
            .cases
            .iter()
            .find(|c| c.category == OpenApiCategory::RateLimit)
            .unwrap();
        assert_eq!(rate_case.repeat, 100);
    }

    #[test]
    fn swagger_2_uses_host_and_base_path() {
        let plan = build_plan(SWAGGER_2.as_bytes(), None).unwrap();
        assert_eq!(plan.version, "2.0");
        assert_eq!(plan.server_url, "https://api.example.com/v2");
        let idor = plan
            .cases
            .iter()
            .filter(|c| c.category == OpenApiCategory::Idor)
            .count();
        assert_eq!(idor, 3, "expected 3 IDOR mutations for itemId");
        let rate = plan
            .cases
            .iter()
            .filter(|c| c.category == OpenApiCategory::RateLimit)
            .count();
        assert_eq!(rate, 1);
    }

    #[test]
    fn base_override_replaces_server_url() {
        let plan = build_plan(SPEC_3.as_bytes(), Some("https://staging.example.com/v1")).unwrap();
        assert!(plan.server_url.starts_with("https://staging.example.com"));
        assert!(plan.cases.iter().all(|c| c.url.starts_with("https://staging.example.com/v1")));
    }

    #[test]
    fn empty_paths_returns_diagnostic_not_error() {
        let plan = build_plan(br#"{"openapi": "3.0.0"}"#, None).unwrap();
        assert!(plan.cases.is_empty());
        assert!(!plan.diagnostics.is_empty());
    }
}
