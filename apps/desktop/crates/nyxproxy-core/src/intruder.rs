//! Intruder — automated request fuzzer.
//!
//! Implements all four Burp-style attack modes:
//!
//! - **Sniper** (one payload set, multiple markers): for every marker position
//!   `i` and every payload `p`, replace position `i` with `p` and leave the
//!   other markers at their default (the text between the `§…§` pair).
//!   Total attempts = `#positions * #payloads`.
//! - **Battering ram** (one payload set, multiple markers): for every payload
//!   `p`, replace *all* marker positions with `p`. Total = `#payloads`.
//! - **Pitchfork** (N payload sets, one per marker): zip the payload sets and
//!   emit one attempt per tuple. Total = `min(set_size)`.
//! - **Cluster bomb** (N payload sets, one per marker): Cartesian product of
//!   the payload sets. Total = `Π set_size`.
//!
//! Markers in the request template use Burp's `§default§` syntax. The text
//! between the two `§` characters is the position's default value (preserved
//! when other positions are being mutated in Sniper mode).

use std::time::Instant;

use base64::Engine;
use futures_util::stream::{self, Stream, StreamExt};
use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::error::{NyxError, NyxResult};
use crate::model::{CapturedRequest, HeaderEntry};

pub const MARKER: char = '§';

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttackType {
    Sniper,
    BatteringRam,
    Pitchfork,
    ClusterBomb,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntruderConfig {
    pub template: CapturedRequest,
    /// One payload set per marker position. Sniper & battering-ram only use
    /// `payload_sets[0]`. Pitchfork & cluster-bomb expect exactly one set per
    /// detected marker position; if fewer sets are provided the extras reuse
    /// the last set.
    pub payload_sets: Vec<Vec<String>>,
    pub attack: AttackType,
    #[serde(default = "default_concurrency")]
    pub concurrency: usize,
    #[serde(default)]
    pub insecure: bool,
}

fn default_concurrency() -> usize {
    8
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntruderAttempt {
    pub index: usize,
    /// One payload per marker position (in marker order).
    pub payloads: Vec<String>,
    pub status: Option<u16>,
    pub response_length: Option<usize>,
    pub elapsed_ms: u64,
    pub error: Option<String>,
    pub snippet: Option<String>,
}

/// A piece of a parsed template — either fixed text or a numbered marker
/// position referencing the parent template's `defaults` slot.
#[derive(Debug, Clone)]
enum TemplatePart {
    Literal(String),
    Position(usize),
}

#[derive(Debug, Clone)]
struct ParsedField {
    parts: Vec<TemplatePart>,
}

#[derive(Debug, Clone)]
struct ParsedTemplate {
    method: String,
    url: ParsedField,
    headers: Vec<(ParsedField, ParsedField)>,
    body: ParsedField,
    defaults: Vec<String>,
}

/// Parse a string into a (parts, default-values) pair, consuming markers from
/// the shared counter so position numbers are globally consistent across the
/// whole request template.
fn parse_field(input: &str, defaults: &mut Vec<String>) -> ParsedField {
    let mut parts = Vec::new();
    let mut buf = String::new();
    let mut chars = input.chars();
    while let Some(c) = chars.next() {
        if c == MARKER {
            if !buf.is_empty() {
                parts.push(TemplatePart::Literal(std::mem::take(&mut buf)));
            }
            let mut default = String::new();
            for c2 in chars.by_ref() {
                if c2 == MARKER {
                    break;
                }
                default.push(c2);
            }
            let idx = defaults.len();
            defaults.push(default);
            parts.push(TemplatePart::Position(idx));
        } else {
            buf.push(c);
        }
    }
    if !buf.is_empty() {
        parts.push(TemplatePart::Literal(buf));
    }
    ParsedField { parts }
}

fn parse_template(template: &CapturedRequest) -> NyxResult<ParsedTemplate> {
    let mut defaults = Vec::new();
    let url = parse_field(&template.url, &mut defaults);
    let headers = template
        .headers
        .iter()
        .map(|h| {
            (
                parse_field(&h.name, &mut defaults),
                parse_field(&h.value, &mut defaults),
            )
        })
        .collect();
    let body_bytes = base64::engine::general_purpose::STANDARD
        .decode(template.body_b64.as_bytes())
        .map_err(|e| NyxError::BadRequest(format!("body base64: {e}")))?;
    let body_str = String::from_utf8(body_bytes)
        .map_err(|e| NyxError::BadRequest(format!("body utf8: {e}")))?;
    let body = parse_field(&body_str, &mut defaults);
    Ok(ParsedTemplate {
        method: template.method.clone(),
        url,
        headers,
        body,
        defaults,
    })
}

fn render_field(field: &ParsedField, values: &[String]) -> String {
    let mut out = String::new();
    for p in &field.parts {
        match p {
            TemplatePart::Literal(s) => out.push_str(s),
            TemplatePart::Position(i) => {
                if let Some(v) = values.get(*i) {
                    out.push_str(v);
                }
            }
        }
    }
    out
}

fn render(template: &ParsedTemplate, values: &[String]) -> (String, String, Vec<HeaderEntry>, Vec<u8>) {
    let url = render_field(&template.url, values);
    let headers = template
        .headers
        .iter()
        .map(|(n, v)| HeaderEntry::new(render_field(n, values), render_field(v, values)))
        .collect();
    let body = render_field(&template.body, values).into_bytes();
    (template.method.clone(), url, headers, body)
}

/// Build the sequence of (position-payload-vector) tuples that the attack
/// mode will fire. Returns an empty vec when the configuration is degenerate
/// (no markers + non-sniper, or no payloads provided).
fn build_attempts(attack: AttackType, defaults: &[String], sets: &[Vec<String>]) -> Vec<Vec<String>> {
    let n_positions = defaults.len();
    if n_positions == 0 {
        return Vec::new();
    }
    match attack {
        AttackType::Sniper => {
            let payloads = sets.first().cloned().unwrap_or_default();
            let mut out = Vec::with_capacity(n_positions * payloads.len());
            for pos in 0..n_positions {
                for payload in &payloads {
                    let mut values: Vec<String> = defaults.to_vec();
                    values[pos] = payload.clone();
                    out.push(values);
                }
            }
            out
        }
        AttackType::BatteringRam => {
            let payloads = sets.first().cloned().unwrap_or_default();
            payloads
                .into_iter()
                .map(|p| vec![p; n_positions])
                .collect()
        }
        AttackType::Pitchfork => {
            if sets.is_empty() {
                return Vec::new();
            }
            // Pad the sets to one per position by re-using the last set.
            let padded: Vec<&Vec<String>> = (0..n_positions)
                .map(|i| sets.get(i).or_else(|| sets.last()).unwrap())
                .collect();
            let min_len = padded.iter().map(|s| s.len()).min().unwrap_or(0);
            (0..min_len)
                .map(|row| {
                    padded
                        .iter()
                        .map(|set| set[row].clone())
                        .collect::<Vec<_>>()
                })
                .collect()
        }
        AttackType::ClusterBomb => {
            if sets.is_empty() {
                return Vec::new();
            }
            let padded: Vec<&Vec<String>> = (0..n_positions)
                .map(|i| sets.get(i).or_else(|| sets.last()).unwrap())
                .collect();
            if padded.iter().any(|s| s.is_empty()) {
                return Vec::new();
            }
            let mut indices = vec![0usize; n_positions];
            let mut out = Vec::new();
            loop {
                let row: Vec<String> = padded
                    .iter()
                    .zip(indices.iter())
                    .map(|(set, idx)| set[*idx].clone())
                    .collect();
                out.push(row);

                // Bump the last counter, carrying as we overflow each set.
                let mut carry = true;
                for i in (0..n_positions).rev() {
                    if !carry {
                        break;
                    }
                    indices[i] += 1;
                    if indices[i] >= padded[i].len() {
                        indices[i] = 0;
                    } else {
                        carry = false;
                    }
                }
                if carry {
                    break;
                }
            }
            out
        }
    }
}

pub fn run<'a>(cfg: &'a IntruderConfig) -> impl Stream<Item = IntruderAttempt> + 'a {
    let parsed = match parse_template(&cfg.template) {
        Ok(p) => p,
        Err(e) => {
            return Box::pin(stream::once(async move {
                IntruderAttempt {
                    index: 0,
                    payloads: Vec::new(),
                    status: None,
                    response_length: None,
                    elapsed_ms: 0,
                    error: Some(e.to_string()),
                    snippet: None,
                }
            })) as std::pin::Pin<Box<dyn Stream<Item = IntruderAttempt> + Send + 'a>>;
        }
    };
    let attempts = build_attempts(cfg.attack, &parsed.defaults, &cfg.payload_sets);

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

    let stream = stream::iter(attempts.into_iter().enumerate())
        .map(move |(index, values)| {
            let client = client.clone();
            let parsed = parsed.clone();
            async move {
                let start = Instant::now();
                let (method, url, headers, body) = render(&parsed, &values);
                match execute_single(&client, &method, &url, &headers, body).await {
                    Ok((status, len, snippet)) => IntruderAttempt {
                        index,
                        payloads: values,
                        status: Some(status),
                        response_length: Some(len),
                        elapsed_ms: start.elapsed().as_millis() as u64,
                        error: None,
                        snippet: Some(snippet),
                    },
                    Err(err) => IntruderAttempt {
                        index,
                        payloads: values,
                        status: None,
                        response_length: None,
                        elapsed_ms: start.elapsed().as_millis() as u64,
                        error: Some(err.to_string()),
                        snippet: None,
                    },
                }
            }
        })
        .buffer_unordered(concurrency);
    Box::pin(stream) as std::pin::Pin<Box<dyn Stream<Item = IntruderAttempt> + Send + 'a>>
}

async fn execute_single(
    client: &Client,
    method: &str,
    url: &str,
    headers: &[HeaderEntry],
    body: Vec<u8>,
) -> NyxResult<(u16, usize, String)> {
    let method = reqwest::Method::from_bytes(method.as_bytes())
        .map_err(|e| NyxError::BadRequest(format!("invalid method: {e}")))?;
    let mut req = client.request(method, url);
    for header in headers {
        let lower = header.name.to_ascii_lowercase();
        if matches!(
            lower.as_str(),
            "host" | "content-length" | "connection" | "transfer-encoding"
        ) {
            continue;
        }
        req = req.header(&header.name, &header.value);
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

    fn template_two_markers() -> CapturedRequest {
        CapturedRequest {
            method: "GET".into(),
            url: "https://example.invalid/search?q=§foo§&filter=§active§".into(),
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
    fn parse_field_extracts_defaults() {
        let mut defaults = Vec::new();
        let field = parse_field("a=§one§&b=§two§", &mut defaults);
        assert_eq!(defaults, vec!["one".to_string(), "two".to_string()]);
        let rendered = render_field(&field, &["X".into(), "Y".into()]);
        assert_eq!(rendered, "a=X&b=Y");
    }

    #[test]
    fn parse_template_collects_all_markers() {
        let parsed = parse_template(&template_two_markers()).unwrap();
        assert_eq!(parsed.defaults, vec!["foo".to_string(), "active".to_string()]);
    }

    #[test]
    fn sniper_keeps_other_positions_default() {
        let parsed = parse_template(&template_two_markers()).unwrap();
        let attempts =
            build_attempts(AttackType::Sniper, &parsed.defaults, &[vec!["x".into(), "y".into()]]);
        // 2 positions × 2 payloads = 4 attempts.
        assert_eq!(attempts.len(), 4);
        assert_eq!(attempts[0], vec!["x".to_string(), "active".to_string()]);
        assert_eq!(attempts[1], vec!["y".to_string(), "active".to_string()]);
        assert_eq!(attempts[2], vec!["foo".to_string(), "x".to_string()]);
        assert_eq!(attempts[3], vec!["foo".to_string(), "y".to_string()]);
    }

    #[test]
    fn battering_ram_writes_same_payload_everywhere() {
        let parsed = parse_template(&template_two_markers()).unwrap();
        let attempts =
            build_attempts(AttackType::BatteringRam, &parsed.defaults, &[vec!["a".into(), "b".into()]]);
        assert_eq!(attempts.len(), 2);
        assert_eq!(attempts[0], vec!["a".to_string(), "a".to_string()]);
        assert_eq!(attempts[1], vec!["b".to_string(), "b".to_string()]);
    }

    #[test]
    fn pitchfork_zips_sets() {
        let parsed = parse_template(&template_two_markers()).unwrap();
        let sets = vec![
            vec!["u1".into(), "u2".into(), "u3".into()],
            vec!["p1".into(), "p2".into()],
        ];
        let attempts = build_attempts(AttackType::Pitchfork, &parsed.defaults, &sets);
        // Min length wins.
        assert_eq!(attempts.len(), 2);
        assert_eq!(attempts[0], vec!["u1".to_string(), "p1".to_string()]);
        assert_eq!(attempts[1], vec!["u2".to_string(), "p2".to_string()]);
    }

    #[test]
    fn cluster_bomb_is_cartesian() {
        let parsed = parse_template(&template_two_markers()).unwrap();
        let sets = vec![
            vec!["a".into(), "b".into()],
            vec!["1".into(), "2".into(), "3".into()],
        ];
        let attempts = build_attempts(AttackType::ClusterBomb, &parsed.defaults, &sets);
        assert_eq!(attempts.len(), 6);
        assert_eq!(attempts[0], vec!["a".to_string(), "1".to_string()]);
        assert_eq!(attempts[1], vec!["a".to_string(), "2".to_string()]);
        assert_eq!(attempts[2], vec!["a".to_string(), "3".to_string()]);
        assert_eq!(attempts[3], vec!["b".to_string(), "1".to_string()]);
        assert_eq!(attempts[4], vec!["b".to_string(), "2".to_string()]);
        assert_eq!(attempts[5], vec!["b".to_string(), "3".to_string()]);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn run_emits_one_attempt_per_payload() {
        let cfg = IntruderConfig {
            template: template_two_markers(),
            payload_sets: vec![vec!["a".into(), "b".into(), "c".into()]],
            attack: AttackType::BatteringRam,
            concurrency: 4,
            insecure: false,
        };
        let attempts = run_to_completion(&cfg).await;
        assert_eq!(attempts.len(), 3);
        // Every attempt should resolve (with an error or status), not get lost.
        for a in &attempts {
            assert_eq!(a.payloads.len(), 2);
        }
    }
}
