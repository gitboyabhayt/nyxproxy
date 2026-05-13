//! Passive scanner — rule-based analyser that flags suspicious request /
//! response patterns and converts them into actionable issues. Rules run on
//! every captured flow without modifying it (passive). An AI fallback can be
//! invoked on demand for deeper review of individual flows.

use std::collections::HashMap;

use base64::Engine;
use serde::{Deserialize, Serialize};

use crate::model::{CapturedRequest, CapturedResponse, HttpFlow};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum IssueSeverity {
    Info,
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum IssueConfidence {
    Tentative,
    Firm,
    Certain,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Issue {
    pub id: String,
    pub flow_id: String,
    pub rule_id: String,
    pub name: String,
    pub severity: IssueSeverity,
    pub confidence: IssueConfidence,
    pub description: String,
    pub evidence: Option<String>,
    pub remediation: Option<String>,
    pub host: String,
    pub path: String,
}

/// Run every passive rule against a single flow. Issues are deterministically
/// keyed by `{rule_id}|{flow_id}|{host}|{path}` so re-scanning the same flow
/// does not produce duplicates.
pub fn scan(flow: &HttpFlow) -> Vec<Issue> {
    let mut out = Vec::new();
    for rule in passive_rules() {
        for issue in rule(flow) {
            out.push(issue);
        }
    }
    out
}

type Rule = fn(&HttpFlow) -> Vec<Issue>;

fn passive_rules() -> &'static [Rule] {
    &[
        rule_missing_security_headers,
        rule_set_cookie_flags,
        rule_information_disclosure,
        rule_directory_listing,
        rule_dangerous_methods,
        rule_password_in_url,
        rule_mixed_content,
        rule_cors_wildcard,
        rule_basic_auth_over_http,
        rule_server_banner,
        rule_jwt_alg_none,
        rule_open_redirect_hint,
    ]
}

fn make_issue(
    flow: &HttpFlow,
    rule_id: &str,
    name: &str,
    severity: IssueSeverity,
    confidence: IssueConfidence,
    description: &str,
    evidence: Option<String>,
    remediation: Option<&str>,
) -> Issue {
    Issue {
        id: format!(
            "{}|{}|{}|{}",
            rule_id, flow.id, flow.request.authority, flow.request.path
        ),
        flow_id: flow.id.to_string(),
        rule_id: rule_id.into(),
        name: name.into(),
        severity,
        confidence,
        description: description.into(),
        evidence,
        remediation: remediation.map(|s| s.into()),
        host: flow.request.authority.clone(),
        path: flow.request.path.clone(),
    }
}

fn header_lookup(headers: &[crate::model::HeaderEntry]) -> HashMap<String, String> {
    headers
        .iter()
        .map(|h| (h.name.to_ascii_lowercase(), h.value.clone()))
        .collect()
}

fn rule_missing_security_headers(flow: &HttpFlow) -> Vec<Issue> {
    let Some(resp) = flow.response.as_ref() else {
        return Vec::new();
    };
    if !is_html(resp) {
        return Vec::new();
    }
    let headers = header_lookup(&resp.headers);
    let mut missing = Vec::new();
    if !headers.contains_key("content-security-policy") {
        missing.push("Content-Security-Policy");
    }
    if !headers.contains_key("strict-transport-security") && flow.request.scheme == "https" {
        missing.push("Strict-Transport-Security");
    }
    if !headers.contains_key("x-frame-options")
        && !headers
            .get("content-security-policy")
            .map(|v| v.contains("frame-ancestors"))
            .unwrap_or(false)
    {
        missing.push("X-Frame-Options or CSP frame-ancestors");
    }
    if !headers.contains_key("x-content-type-options") {
        missing.push("X-Content-Type-Options");
    }
    if !headers.contains_key("referrer-policy") {
        missing.push("Referrer-Policy");
    }

    if missing.is_empty() {
        return Vec::new();
    }
    vec![make_issue(
        flow,
        "missing-security-headers",
        "Missing security response headers",
        IssueSeverity::Low,
        IssueConfidence::Firm,
        "Response is missing one or more recommended security headers. \
         These headers help mitigate XSS, clickjacking, MIME sniffing and \
         leaked referrers.",
        Some(missing.join(", ")),
        Some(
            "Set the missing headers in your web server / framework. \
             Use a strict Content-Security-Policy where possible.",
        ),
    )]
}

fn rule_set_cookie_flags(flow: &HttpFlow) -> Vec<Issue> {
    let Some(resp) = flow.response.as_ref() else {
        return Vec::new();
    };
    let mut issues = Vec::new();
    for h in &resp.headers {
        if !h.name.eq_ignore_ascii_case("set-cookie") {
            continue;
        }
        let value = &h.value;
        let lowered = value.to_ascii_lowercase();
        let mut missing_flags = Vec::new();
        if !lowered.contains("secure") && flow.request.scheme == "https" {
            missing_flags.push("Secure");
        }
        if !lowered.contains("httponly") {
            missing_flags.push("HttpOnly");
        }
        if !lowered.contains("samesite") {
            missing_flags.push("SameSite");
        }
        if !missing_flags.is_empty() {
            issues.push(make_issue(
                flow,
                "cookie-flags",
                "Cookie missing security flags",
                IssueSeverity::Medium,
                IssueConfidence::Firm,
                "One or more cookies are set without recommended attributes \
                 that protect them from JS exposure, transport sniffing or \
                 cross-site issuance.",
                Some(format!("{}: missing {}", value, missing_flags.join(", "))),
                Some(
                    "Add Secure (HTTPS), HttpOnly (non-JS) and SameSite=Lax/Strict \
                     attributes to session cookies.",
                ),
            ));
        }
    }
    issues
}

fn rule_information_disclosure(flow: &HttpFlow) -> Vec<Issue> {
    let Some(resp) = flow.response.as_ref() else {
        return Vec::new();
    };
    let mut issues = Vec::new();
    let body = decode_body(resp);
    let lower = body.to_ascii_lowercase();
    let needles = [
        ("stack trace", "Stack trace exposed"),
        ("traceback (most recent call last)", "Python traceback exposed"),
        ("syntaxerror:", "Server stack exposed"),
        ("php fatal error", "PHP error exposed"),
        ("ora-", "Oracle error exposed"),
        ("sqlstate", "SQL state exposed"),
        ("mysql_fetch", "MySQL error exposed"),
        ("postgresql:", "PostgreSQL message exposed"),
        ("aws_access_key_id", "AWS access key exposed"),
        ("-----begin private key-----", "Private key exposed"),
        ("-----begin rsa private key-----", "RSA private key exposed"),
    ];
    for (needle, label) in needles {
        if lower.contains(needle) {
            issues.push(make_issue(
                flow,
                "info-disclosure",
                label,
                IssueSeverity::High,
                IssueConfidence::Firm,
                "Response body contains content that suggests sensitive server-side \
                 information has leaked to the client.",
                Some(needle.into()),
                Some(
                    "Disable detailed error pages in production and avoid \
                     reflecting internal stack traces or credentials to clients.",
                ),
            ));
        }
    }
    issues
}

fn rule_directory_listing(flow: &HttpFlow) -> Vec<Issue> {
    let Some(resp) = flow.response.as_ref() else {
        return Vec::new();
    };
    if !is_html(resp) {
        return Vec::new();
    }
    let body = decode_body(resp);
    if body.contains("Index of /") || body.contains("<title>Directory listing for") {
        return vec![make_issue(
            flow,
            "directory-listing",
            "Directory listing enabled",
            IssueSeverity::Medium,
            IssueConfidence::Firm,
            "The server returned an automatic directory listing. This typically \
             exposes filenames that should not be enumerable.",
            None,
            Some("Disable autoindex / directory listing in the web server config."),
        )];
    }
    Vec::new()
}

fn rule_dangerous_methods(flow: &HttpFlow) -> Vec<Issue> {
    let method = flow.request.method.to_ascii_uppercase();
    if matches!(method.as_str(), "TRACE" | "TRACK" | "CONNECT") {
        vec![make_issue(
            flow,
            "dangerous-method",
            format!("{method} method enabled").as_str(),
            IssueSeverity::Low,
            IssueConfidence::Firm,
            "Request uses an HTTP method that is rarely required and can be abused \
             for cross-site tracing, header reflection or proxy bypass.",
            Some(method.clone()),
            Some("Restrict the allowed HTTP methods at the web-server level."),
        )]
    } else {
        Vec::new()
    }
}

fn rule_password_in_url(flow: &HttpFlow) -> Vec<Issue> {
    let url = &flow.request.url;
    if let Some(query_idx) = url.find('?') {
        let query = &url[query_idx + 1..];
        let mut hits = Vec::new();
        for pair in query.split('&') {
            let (k, v) = match pair.split_once('=') {
                Some(p) => p,
                None => continue,
            };
            let lk = k.to_ascii_lowercase();
            if lk == "password"
                || lk == "passwd"
                || lk == "pwd"
                || lk == "secret"
                || lk == "api_key"
                || lk == "apikey"
                || lk == "token"
                || lk == "auth"
            {
                hits.push(format!("{k}={v}"));
            }
        }
        if !hits.is_empty() {
            return vec![make_issue(
                flow,
                "sensitive-in-url",
                "Sensitive value in URL query string",
                IssueSeverity::High,
                IssueConfidence::Firm,
                "Secrets transmitted as URL query parameters are written to web-server \
                 access logs, browser history, and Referer headers.",
                Some(hits.join("; ")),
                Some("Use POST bodies or Authorization headers for credentials."),
            )];
        }
    }
    Vec::new()
}

fn rule_mixed_content(flow: &HttpFlow) -> Vec<Issue> {
    let Some(resp) = flow.response.as_ref() else {
        return Vec::new();
    };
    if flow.request.scheme != "https" || !is_html(resp) {
        return Vec::new();
    }
    let body = decode_body(resp);
    if body.contains("src=\"http://") || body.contains("href=\"http://") {
        return vec![make_issue(
            flow,
            "mixed-content",
            "Mixed-content reference inside HTTPS page",
            IssueSeverity::Medium,
            IssueConfidence::Tentative,
            "HTTPS page references an http:// resource. Modern browsers block \
             such resources or report them as mixed content.",
            None,
            Some("Update embedded references to use HTTPS or protocol-relative URLs."),
        )];
    }
    Vec::new()
}

fn rule_cors_wildcard(flow: &HttpFlow) -> Vec<Issue> {
    let Some(resp) = flow.response.as_ref() else {
        return Vec::new();
    };
    let headers = header_lookup(&resp.headers);
    let allow_origin = match headers.get("access-control-allow-origin") {
        Some(v) => v.clone(),
        None => return Vec::new(),
    };
    let allow_creds = headers
        .get("access-control-allow-credentials")
        .map(|v| v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);
    if allow_origin == "*" && allow_creds {
        return vec![make_issue(
            flow,
            "cors-wildcard-creds",
            "Wildcard CORS with credentials",
            IssueSeverity::High,
            IssueConfidence::Firm,
            "The response sets Access-Control-Allow-Origin: * together with \
             Access-Control-Allow-Credentials: true. Browsers will reject this, \
             but the configuration intent is dangerous.",
            Some(allow_origin),
            Some(
                "Either remove the wildcard origin or remove credentials. \
                 Echo a specific origin when credentials are required.",
            ),
        )];
    }
    if allow_origin == "*" {
        return vec![make_issue(
            flow,
            "cors-wildcard",
            "Wildcard CORS origin",
            IssueSeverity::Low,
            IssueConfidence::Firm,
            "Access-Control-Allow-Origin is set to * which allows any origin to read this response.",
            Some(allow_origin),
            Some("Restrict the allowed origins to the ones that legitimately need access."),
        )];
    }
    Vec::new()
}

fn rule_basic_auth_over_http(flow: &HttpFlow) -> Vec<Issue> {
    let headers = header_lookup(&flow.request.headers);
    let Some(authz) = headers.get("authorization") else {
        return Vec::new();
    };
    if !authz.to_ascii_lowercase().starts_with("basic ") {
        return Vec::new();
    }
    if flow.request.scheme != "https" {
        vec![make_issue(
            flow,
            "basic-auth-http",
            "HTTP Basic auth over plaintext HTTP",
            IssueSeverity::High,
            IssueConfidence::Certain,
            "Credentials are sent via HTTP Basic over a non-TLS channel. They can \
             be passively sniffed by anyone on path.",
            Some(authz.clone()),
            Some("Switch the endpoint to HTTPS and rotate the exposed credentials."),
        )]
    } else {
        Vec::new()
    }
}

fn rule_server_banner(flow: &HttpFlow) -> Vec<Issue> {
    let Some(resp) = flow.response.as_ref() else {
        return Vec::new();
    };
    let headers = header_lookup(&resp.headers);
    let mut banners = Vec::new();
    for name in ["server", "x-powered-by", "x-aspnet-version"] {
        if let Some(value) = headers.get(name) {
            banners.push(format!("{name}: {value}"));
        }
    }
    if banners.is_empty() {
        return Vec::new();
    }
    vec![make_issue(
        flow,
        "server-banner",
        "Server / framework banner leaked",
        IssueSeverity::Info,
        IssueConfidence::Firm,
        "Response headers disclose detailed server / framework versions. \
         Attackers can use this to look up known CVEs.",
        Some(banners.join("; ")),
        Some("Suppress or generic-ise these headers in the web-server config."),
    )]
}

fn rule_jwt_alg_none(flow: &HttpFlow) -> Vec<Issue> {
    let headers = header_lookup(&flow.request.headers);
    let Some(authz) = headers.get("authorization") else {
        return Vec::new();
    };
    let lowered = authz.to_ascii_lowercase();
    if !lowered.starts_with("bearer ") {
        return Vec::new();
    }
    let token = &authz[7..];
    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() < 2 {
        return Vec::new();
    }
    let header_b64 = parts[0];
    let pad = (4 - header_b64.len() % 4) % 4;
    let mut padded = header_b64.to_string();
    padded.push_str(&"=".repeat(pad));
    let decoded = match base64::engine::general_purpose::URL_SAFE.decode(padded.as_bytes()) {
        Ok(b) => b,
        Err(_) => return Vec::new(),
    };
    let header_str = String::from_utf8_lossy(&decoded);
    let lowered_hdr = header_str.to_ascii_lowercase();
    if lowered_hdr.contains("\"alg\":\"none\"") || lowered_hdr.contains("\"alg\": \"none\"") {
        return vec![make_issue(
            flow,
            "jwt-alg-none",
            "JWT with alg=none accepted",
            IssueSeverity::Critical,
            IssueConfidence::Certain,
            "The client is sending a JWT whose header declares alg=none. If the \
             server validates such tokens, authentication can be trivially forged.",
            Some(header_str.into_owned()),
            Some("Reject any JWT whose alg is not in the server's allow-list."),
        )];
    }
    Vec::new()
}

fn rule_open_redirect_hint(flow: &HttpFlow) -> Vec<Issue> {
    let url = &flow.request.url;
    let Some(qi) = url.find('?') else {
        return Vec::new();
    };
    let query = &url[qi + 1..];
    let suspicious_keys = ["redirect", "url", "next", "returnurl", "return", "callback"];
    for pair in query.split('&') {
        let Some((k, v)) = pair.split_once('=') else {
            continue;
        };
        if !suspicious_keys.contains(&k.to_ascii_lowercase().as_str()) {
            continue;
        }
        let decoded = urlencoding_decode(v);
        if decoded.starts_with("//") || decoded.to_ascii_lowercase().starts_with("http") {
            return vec![make_issue(
                flow,
                "open-redirect-hint",
                "Possible open-redirect sink",
                IssueSeverity::Medium,
                IssueConfidence::Tentative,
                "A query parameter named like a redirect target carries an absolute URL. \
                 If the server follows the value blindly, this is an open redirect.",
                Some(format!("{k}={decoded}")),
                Some("Validate redirect targets against an allow-list of known paths."),
            )];
        }
    }
    Vec::new()
}

fn urlencoding_decode(input: &str) -> String {
    let mut out = String::new();
    let bytes = input.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or("00");
            if let Ok(b) = u8::from_str_radix(hex, 16) {
                out.push(b as char);
                i += 3;
                continue;
            }
        }
        if bytes[i] == b'+' {
            out.push(' ');
        } else {
            out.push(bytes[i] as char);
        }
        i += 1;
    }
    out
}

fn is_html(resp: &CapturedResponse) -> bool {
    resp.headers
        .iter()
        .find(|h| h.name.eq_ignore_ascii_case("content-type"))
        .map(|h| h.value.to_ascii_lowercase().contains("html"))
        .unwrap_or(false)
}

fn decode_body(resp: &CapturedResponse) -> String {
    let bytes = match base64::engine::general_purpose::STANDARD.decode(resp.body_b64.as_bytes()) {
        Ok(b) => b,
        Err(_) => return String::new(),
    };
    String::from_utf8_lossy(&bytes).into_owned()
}

/// Convenience: scan a CapturedRequest+Response pair directly without wrapping
/// in a full HttpFlow (used by tests and ad-hoc tooling).
pub fn scan_pair(req: CapturedRequest, resp: Option<CapturedResponse>) -> Vec<Issue> {
    let mut flow = HttpFlow::new(req);
    flow.response = resp;
    scan(&flow)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::HeaderEntry;

    fn req(url: &str) -> CapturedRequest {
        let parsed = url::Url::parse(url).unwrap();
        CapturedRequest {
            method: "GET".into(),
            url: url.into(),
            scheme: parsed.scheme().into(),
            authority: parsed.host_str().unwrap_or("").into(),
            path: parsed.path().into(),
            http_version: "HTTP/1.1".into(),
            headers: Vec::new(),
            body_b64: base64::engine::general_purpose::STANDARD.encode(b""),
            body_size: 0,
        }
    }

    fn resp(headers: Vec<HeaderEntry>, body: &[u8]) -> CapturedResponse {
        CapturedResponse {
            status: 200,
            http_version: "HTTP/1.1".into(),
            reason: "OK".into(),
            headers,
            body_b64: base64::engine::general_purpose::STANDARD.encode(body),
            body_size: body.len(),
            elapsed_ms: 0,
        }
    }

    #[test]
    fn flags_missing_security_headers() {
        let issues = scan_pair(
            req("https://example.com/"),
            Some(resp(vec![HeaderEntry::new("Content-Type", "text/html")], b"<html></html>")),
        );
        assert!(issues.iter().any(|i| i.rule_id == "missing-security-headers"));
    }

    #[test]
    fn flags_cookie_without_secure() {
        let issues = scan_pair(
            req("https://example.com/"),
            Some(resp(
                vec![
                    HeaderEntry::new("Set-Cookie", "session=abc"),
                ],
                b"",
            )),
        );
        assert!(issues.iter().any(|i| i.rule_id == "cookie-flags"));
    }

    #[test]
    fn flags_password_in_query() {
        let mut r = req("https://example.com/login?user=a&password=hunter2");
        r.headers.push(HeaderEntry::new("Host", "example.com"));
        let issues = scan_pair(r, None);
        assert!(issues.iter().any(|i| i.rule_id == "sensitive-in-url"));
    }

    #[test]
    fn flags_cors_wildcard_with_creds() {
        let issues = scan_pair(
            req("https://example.com/"),
            Some(resp(
                vec![
                    HeaderEntry::new("Access-Control-Allow-Origin", "*"),
                    HeaderEntry::new("Access-Control-Allow-Credentials", "true"),
                ],
                b"",
            )),
        );
        assert!(issues.iter().any(|i| i.rule_id == "cors-wildcard-creds"));
    }

    #[test]
    fn flags_basic_auth_over_http() {
        let mut r = req("http://example.com/secret");
        r.headers
            .push(HeaderEntry::new("Authorization", "Basic dXNlcjpwYXNz"));
        let issues = scan_pair(r, None);
        assert!(issues.iter().any(|i| i.rule_id == "basic-auth-http"));
    }

    #[test]
    fn flags_jwt_alg_none() {
        let mut r = req("https://example.com/api");
        // {"alg":"none","typ":"JWT"}.payload.
        let header = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(br#"{"alg":"none","typ":"JWT"}"#);
        let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(b"{\"sub\":\"x\"}");
        let token = format!("Bearer {header}.{payload}.");
        r.headers.push(HeaderEntry::new("Authorization", &token));
        let issues = scan_pair(r, None);
        assert!(issues.iter().any(|i| i.rule_id == "jwt-alg-none"));
    }

    #[test]
    fn flags_open_redirect_hint() {
        let issues = scan_pair(
            req("https://example.com/go?next=https%3A%2F%2Fattacker.test%2F"),
            None,
        );
        assert!(issues.iter().any(|i| i.rule_id == "open-redirect-hint"));
    }

    #[test]
    fn flags_information_disclosure_python_traceback() {
        let body = b"Traceback (most recent call last):\n  File 'x.py'";
        let issues = scan_pair(
            req("https://example.com/api"),
            Some(resp(vec![HeaderEntry::new("Content-Type", "text/plain")], body)),
        );
        assert!(issues.iter().any(|i| i.rule_id == "info-disclosure"));
    }

    #[test]
    fn clean_response_has_no_issues_for_redirect_or_disclosure_rules() {
        let issues = scan_pair(
            req("https://example.com/"),
            Some(resp(
                vec![
                    HeaderEntry::new("Content-Security-Policy", "default-src 'self'"),
                    HeaderEntry::new("Strict-Transport-Security", "max-age=31536000"),
                    HeaderEntry::new("X-Frame-Options", "DENY"),
                    HeaderEntry::new("X-Content-Type-Options", "nosniff"),
                    HeaderEntry::new("Referrer-Policy", "no-referrer"),
                    HeaderEntry::new("Content-Type", "text/html"),
                    HeaderEntry::new("Set-Cookie", "s=1; Secure; HttpOnly; SameSite=Strict"),
                ],
                b"<html><body>ok</body></html>",
            )),
        );
        assert!(!issues.iter().any(|i| i.rule_id == "missing-security-headers"));
        assert!(!issues.iter().any(|i| i.rule_id == "cookie-flags"));
        assert!(!issues.iter().any(|i| i.rule_id == "info-disclosure"));
    }
}
