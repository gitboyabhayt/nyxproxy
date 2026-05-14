//! Importer for Burp Suite "Save items" XML exports.
//!
//! Burp Suite Professional / Community lets users select items in Proxy
//! History (or any tool with a history table) and `right-click → Save items`.
//! The resulting `.xml` file has a stable, well-documented structure that has
//! survived from Burp 1.7 through Burp 2024.x. We parse that format and
//! convert each `<item>` into a [`HttpFlow`] so users migrating from Burp can
//! bring years of project history into NyxProxy.
//!
//! Supported fields per `<item>`:
//!
//! ```xml
//! <items burpVersion="2024.8.4" exportTime="...">
//!   <item>
//!     <time>Tue Apr 09 14:32:11 IST 2024</time>
//!     <url><![CDATA[https://example.com/path]]></url>
//!     <host ip="93.184.216.34">example.com</host>
//!     <port>443</port>
//!     <protocol>https</protocol>
//!     <method><![CDATA[GET]]></method>
//!     <path><![CDATA[/path]]></path>
//!     <extension>null</extension>
//!     <request base64="true"><![CDATA[base64-of-raw-request]]></request>
//!     <status>200</status>
//!     <responselength>1234</responselength>
//!     <mimetype>HTML</mimetype>
//!     <response base64="true"><![CDATA[base64-of-raw-response]]></response>
//!     <comment></comment>
//!   </item>
//! </items>
//! ```
//!
//! The raw `<request>` and `<response>` payloads are the full bytes Burp
//! captured on the wire, so we re-parse the HTTP/1.1 framing to split out
//! method/path/version, headers, and body.
//!
//! When `base64="true"` (Burp's default for binary safety) the inner text is
//! base64-decoded first.

use std::io::BufRead;

use base64::Engine;
use chrono::{DateTime, TimeZone, Utc};
use quick_xml::events::Event;
use quick_xml::reader::Reader;
use serde::{Deserialize, Serialize};

use crate::error::{NyxError, NyxResult};
use crate::model::{CapturedRequest, CapturedResponse, HeaderEntry, HttpFlow};

/// Summary of an import run, returned to the UI.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BurpImportSummary {
    pub items_seen: usize,
    pub items_imported: usize,
    pub items_skipped: usize,
    pub errors: Vec<String>,
    pub burp_version: Option<String>,
    pub export_time: Option<String>,
}

#[derive(Debug, Default)]
struct RawItem {
    time: Option<String>,
    url: Option<String>,
    host: Option<String>,
    port: Option<u16>,
    protocol: Option<String>,
    method: Option<String>,
    path: Option<String>,
    request_b64: bool,
    request_raw: Option<String>,
    status: Option<u16>,
    response_b64: bool,
    response_raw: Option<String>,
    comment: Option<String>,
    response_length: Option<usize>,
}

/// Parse a Burp Suite "Save items" XML export and return the flows it contains.
///
/// Items that fail to decode are skipped (collected into `errors`) rather than
/// aborting the import — Burp exports sometimes contain partial / truncated
/// rows from interrupted captures.
pub fn parse_burp_xml(bytes: &[u8]) -> NyxResult<(Vec<HttpFlow>, BurpImportSummary)> {
    let mut reader = Reader::from_reader(bytes);
    reader.config_mut().trim_text(true);

    let mut buf = Vec::new();
    let mut flows = Vec::new();
    let mut summary = BurpImportSummary::default();

    enum State {
        Top,
        Item(RawItem),
        InTag(RawItem, String, bool /* b64 */),
    }
    let mut state = State::Top;

    loop {
        match reader.read_event_into(&mut buf) {
            Err(e) => {
                return Err(NyxError::BadRequest(format!("xml parse error: {e}")));
            }
            Ok(Event::Eof) => break,

            Ok(Event::Start(ref e)) => {
                let name_bytes = e.name();
                let name = String::from_utf8_lossy(name_bytes.as_ref()).to_string();
                match (&mut state, name.as_str()) {
                    (s @ State::Top, "items") => {
                        for attr in e.attributes().flatten() {
                            let key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
                            let val = attr.unescape_value().unwrap_or_default().to_string();
                            if key == "burpVersion" {
                                summary.burp_version = Some(val);
                            } else if key == "exportTime" {
                                summary.export_time = Some(val);
                            }
                        }
                        let _ = s; // remain in Top
                    }
                    (State::Top, "item") => {
                        state = State::Item(RawItem::default());
                    }
                    (State::Item(item), tag) => {
                        let mut b64 = false;
                        for attr in e.attributes().flatten() {
                            let key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
                            if key == "base64" {
                                let v = attr.unescape_value().unwrap_or_default();
                                if v.eq_ignore_ascii_case("true") {
                                    b64 = true;
                                }
                            }
                        }
                        let item = std::mem::take(item);
                        state = State::InTag(item, tag.to_string(), b64);
                    }
                    _ => {}
                }
            }

            Ok(Event::End(ref e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                if name == "item" {
                    if let State::Item(item) = std::mem::replace(&mut state, State::Top) {
                        summary.items_seen += 1;
                        match build_flow(item) {
                            Ok(flow) => {
                                summary.items_imported += 1;
                                flows.push(flow);
                            }
                            Err(err) => {
                                summary.items_skipped += 1;
                                summary.errors.push(err.to_string());
                            }
                        }
                    }
                } else if let State::InTag(item, _, _) = std::mem::replace(&mut state, State::Top) {
                    state = State::Item(item);
                }
            }

            Ok(Event::Text(ref e)) => {
                if let State::InTag(item, tag, b64) = &mut state {
                    let raw = e.unescape().unwrap_or_default().to_string();
                    let decoded = if *b64 {
                        match base64::engine::general_purpose::STANDARD.decode(raw.trim()) {
                            Ok(b) => String::from_utf8_lossy(&b).into_owned(),
                            Err(_) => raw,
                        }
                    } else {
                        raw
                    };
                    merge_tag(item, tag.as_str(), &decoded);
                }
            }

            Ok(Event::CData(ref e)) => {
                if let State::InTag(item, tag, b64) = &mut state {
                    let raw = String::from_utf8_lossy(e.as_ref()).to_string();
                    let decoded = if *b64 {
                        match base64::engine::general_purpose::STANDARD.decode(raw.trim()) {
                            Ok(b) => String::from_utf8_lossy(&b).into_owned(),
                            Err(_) => raw,
                        }
                    } else {
                        raw
                    };
                    merge_tag(item, tag.as_str(), &decoded);
                }
            }

            Ok(_) => {}
        }
        buf.clear();
    }

    Ok((flows, summary))
}

fn merge_tag(item: &mut RawItem, tag: &str, text: &str) {
    match tag {
        "time" => item.time = Some(text.to_string()),
        "url" => item.url = Some(text.to_string()),
        "host" => item.host = Some(text.to_string()),
        "port" => item.port = text.trim().parse().ok(),
        "protocol" => item.protocol = Some(text.to_string()),
        "method" => item.method = Some(text.to_string()),
        "path" => item.path = Some(text.to_string()),
        "request" => {
            item.request_raw = Some(text.to_string());
            item.request_b64 = true;
        }
        "status" => item.status = text.trim().parse().ok(),
        "response" => {
            item.response_raw = Some(text.to_string());
            item.response_b64 = true;
        }
        "comment" => item.comment = Some(text.to_string()),
        "responselength" => item.response_length = text.trim().parse().ok(),
        _ => {}
    }
}

fn build_flow(item: RawItem) -> NyxResult<HttpFlow> {
    let url = item
        .url
        .clone()
        .ok_or_else(|| NyxError::BadRequest("burp item missing <url>".into()))?;
    let scheme = item
        .protocol
        .clone()
        .unwrap_or_else(|| if item.port == Some(443) { "https".into() } else { "http".into() });
    let port = item.port.unwrap_or(if scheme == "https" { 443 } else { 80 });
    let host = item.host.clone().unwrap_or_default();
    let authority = if (scheme == "https" && port == 443) || (scheme == "http" && port == 80) {
        host.clone()
    } else {
        format!("{host}:{port}")
    };

    let raw_request = item.request_raw.unwrap_or_default();
    let (req_method, req_path, req_version, req_headers, req_body) = if raw_request.is_empty() {
        (
            item.method.clone().unwrap_or_else(|| "GET".into()),
            item.path.clone().unwrap_or_else(|| "/".into()),
            "HTTP/1.1".into(),
            Vec::new(),
            Vec::new(),
        )
    } else {
        parse_http_message(raw_request.as_bytes(), MessageKind::Request)?
    };

    let request = CapturedRequest {
        method: req_method,
        url,
        scheme: scheme.clone(),
        authority,
        path: req_path,
        http_version: req_version,
        headers: req_headers,
        body_b64: base64::engine::general_purpose::STANDARD.encode(&req_body),
        body_size: req_body.len(),
    };

    let mut flow = HttpFlow::new(request);
    flow.tags.push("import:burp".into());
    if let Some(comment) = item.comment.filter(|s| !s.is_empty()) {
        flow.tags.push(format!("comment:{comment}"));
    }

    // Best-effort timestamp parsing.
    if let Some(ts) = item.time.as_deref().and_then(parse_burp_time) {
        flow.started_at = ts;
    }

    let raw_response = item.response_raw.unwrap_or_default();
    if !raw_response.is_empty() {
        let (_method, _path, version, headers, body) =
            parse_http_message(raw_response.as_bytes(), MessageKind::Response)?;
        let status = item.status.unwrap_or(0);
        let reason = ""; // Burp doesn't preserve the reason phrase separately; we leave it blank.
        flow.response = Some(CapturedResponse {
            status,
            http_version: version,
            reason: reason.to_string(),
            headers,
            body_size: item.response_length.unwrap_or(body.len()),
            body_b64: base64::engine::general_purpose::STANDARD.encode(&body),
            elapsed_ms: 0,
        });
    }

    Ok(flow)
}

#[derive(Copy, Clone)]
enum MessageKind {
    Request,
    Response,
}

/// Split a raw HTTP/1.1 message into method/path/version, headers, and body.
///
/// We don't validate against RFC 7230 strictly — Burp captures everything the
/// peer sent, including non-compliant junk. We just split on the first CRLFCRLF
/// (or LFLF) and parse line-by-line.
fn parse_http_message(
    bytes: &[u8],
    kind: MessageKind,
) -> NyxResult<(String, String, String, Vec<HeaderEntry>, Vec<u8>)> {
    let split_at = find_header_terminator(bytes)
        .ok_or_else(|| NyxError::BadRequest("could not locate end of headers".into()))?;
    let (head, rest) = bytes.split_at(split_at.0);
    let body = rest[split_at.1..].to_vec();

    let mut lines = head.lines();
    let start_line = lines
        .next()
        .ok_or_else(|| NyxError::BadRequest("empty http message".into()))?
        .map_err(|e| NyxError::BadRequest(format!("invalid utf-8 in start line: {e}")))?;

    let (method, path, version) = match kind {
        MessageKind::Request => {
            let mut parts = start_line.splitn(3, ' ');
            let m = parts.next().unwrap_or("GET").to_string();
            let p = parts.next().unwrap_or("/").to_string();
            let v = parts.next().unwrap_or("HTTP/1.1").to_string();
            (m, p, v)
        }
        MessageKind::Response => {
            let mut parts = start_line.splitn(3, ' ');
            let v = parts.next().unwrap_or("HTTP/1.1").to_string();
            let _status = parts.next().unwrap_or("0").to_string();
            let _reason = parts.next().unwrap_or("").to_string();
            (String::new(), String::new(), v)
        }
    };

    let mut headers = Vec::new();
    for line in lines {
        let line = match line {
            Ok(l) => l,
            Err(_) => continue,
        };
        if line.is_empty() {
            break;
        }
        if let Some((name, value)) = line.split_once(':') {
            headers.push(HeaderEntry::new(name.trim(), value.trim()));
        }
    }

    Ok((method, path, version, headers, body))
}

/// Find the byte offset where headers end. Returns `(end_of_headers, body_offset_within_rest)`.
fn find_header_terminator(bytes: &[u8]) -> Option<(usize, usize)> {
    if let Some(idx) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
        return Some((idx, 4));
    }
    if let Some(idx) = bytes.windows(2).position(|w| w == b"\n\n") {
        return Some((idx, 2));
    }
    // Whole message was just headers, no terminator (rare).
    Some((bytes.len(), 0))
}

/// Burp timestamps look like `Tue Apr 09 14:32:11 IST 2024`. We do a best-effort
/// parse and fall back to `None` when unsupported.
fn parse_burp_time(s: &str) -> Option<DateTime<Utc>> {
    // RFC 2822-ish, but Burp uses 3-letter TZ which chrono cannot parse without
    // a name table. Try the most common formats; otherwise give up.
    let formats = [
        "%a %b %d %H:%M:%S %Z %Y",
        "%a %b %d %H:%M:%S %Y",
        "%Y-%m-%d %H:%M:%S",
        "%Y-%m-%dT%H:%M:%S%.fZ",
    ];
    for fmt in formats {
        if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(s, fmt) {
            return Some(Utc.from_utc_datetime(&dt));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn b64(s: &str) -> String {
        base64::engine::general_purpose::STANDARD.encode(s.as_bytes())
    }

    fn sample_xml(request: &str, response: &str) -> String {
        format!(
            r#"<?xml version="1.0"?>
<items burpVersion="2024.8.4" exportTime="Tue Apr 09 14:32:11 IST 2024">
  <item>
    <time>Tue Apr 09 14:32:11 IST 2024</time>
    <url><![CDATA[https://example.com/api/users?id=42]]></url>
    <host ip="93.184.216.34">example.com</host>
    <port>443</port>
    <protocol>https</protocol>
    <method><![CDATA[GET]]></method>
    <path><![CDATA[/api/users?id=42]]></path>
    <extension>null</extension>
    <request base64="true"><![CDATA[{}]]></request>
    <status>200</status>
    <responselength>123</responselength>
    <mimetype>JSON</mimetype>
    <response base64="true"><![CDATA[{}]]></response>
    <comment>seen on login flow</comment>
  </item>
</items>"#,
            b64(request),
            b64(response),
        )
    }

    #[test]
    fn parses_minimal_burp_item() {
        let raw_req = "GET /api/users?id=42 HTTP/1.1\r\nHost: example.com\r\nUser-Agent: Burp\r\n\r\n";
        let raw_resp = "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 13\r\n\r\n{\"id\":42}\r\n";
        let xml = sample_xml(raw_req, raw_resp);

        let (flows, summary) = parse_burp_xml(xml.as_bytes()).expect("parse ok");
        assert_eq!(summary.items_seen, 1);
        assert_eq!(summary.items_imported, 1);
        assert_eq!(summary.items_skipped, 0);
        assert_eq!(summary.burp_version.as_deref(), Some("2024.8.4"));

        assert_eq!(flows.len(), 1);
        let flow = &flows[0];
        assert_eq!(flow.request.method, "GET");
        assert_eq!(flow.request.path, "/api/users?id=42");
        assert_eq!(flow.request.http_version, "HTTP/1.1");
        assert!(flow
            .request
            .headers
            .iter()
            .any(|h| h.name.eq_ignore_ascii_case("host") && h.value == "example.com"));
        assert!(flow.tags.iter().any(|t| t == "import:burp"));
        assert!(flow.tags.iter().any(|t| t.contains("comment:seen on login flow")));

        let resp = flow.response.as_ref().expect("response present");
        assert_eq!(resp.status, 200);
        assert!(resp
            .headers
            .iter()
            .any(|h| h.name.eq_ignore_ascii_case("content-type") && h.value == "application/json"));
    }

    #[test]
    fn imports_multiple_items_and_reports_summary() {
        let raw_req = "POST /login HTTP/1.1\r\nHost: example.com\r\nContent-Length: 4\r\n\r\nhiyo";
        let raw_resp = "HTTP/1.1 302 Found\r\nLocation: /home\r\n\r\n";
        let xml = format!(
            r#"<?xml version="1.0"?>
<items burpVersion="2024.8.4">
  {0}
  {0}
  {0}
</items>"#,
            format!(
                r#"<item><url>https://example.com/login</url><host>example.com</host><port>443</port>
<protocol>https</protocol><method>POST</method><path>/login</path>
<request base64="true">{}</request><status>302</status>
<response base64="true">{}</response><comment></comment></item>"#,
                b64(raw_req),
                b64(raw_resp),
            ),
        );

        let (flows, summary) = parse_burp_xml(xml.as_bytes()).expect("parse ok");
        assert_eq!(summary.items_seen, 3);
        assert_eq!(summary.items_imported, 3);
        assert_eq!(flows.len(), 3);
        for f in &flows {
            assert_eq!(f.request.method, "POST");
            assert_eq!(f.response.as_ref().unwrap().status, 302);
        }
    }

    #[test]
    fn handles_lf_only_line_separators_in_request_body() {
        // Some Burp exports normalise CRLF to LF inside <request>. Make sure we still parse.
        let raw_req = "GET / HTTP/1.1\nHost: example.com\n\n";
        let raw_resp = "HTTP/1.1 200 OK\nContent-Length: 0\n\n";
        let xml = sample_xml(raw_req, raw_resp);
        let (flows, _) = parse_burp_xml(xml.as_bytes()).expect("parse ok");
        assert_eq!(flows.len(), 1);
        assert_eq!(flows[0].request.method, "GET");
    }

    #[test]
    fn skips_malformed_item_but_keeps_good_one() {
        let raw_req = "GET / HTTP/1.1\r\nHost: example.com\r\n\r\n";
        let raw_resp = "HTTP/1.1 200 OK\r\n\r\n";
        let xml = format!(
            r#"<?xml version="1.0"?>
<items burpVersion="2024.8.4">
  <item>
    <protocol>https</protocol>
    <method>GET</method>
    <!-- url intentionally missing -->
    <request base64="true">{}</request>
    <response base64="true">{}</response>
  </item>
  <item>
    <url>https://example.com/</url>
    <host>example.com</host>
    <port>443</port>
    <protocol>https</protocol>
    <method>GET</method>
    <request base64="true">{}</request>
    <response base64="true">{}</response>
  </item>
</items>"#,
            b64(raw_req),
            b64(raw_resp),
            b64(raw_req),
            b64(raw_resp),
        );

        let (flows, summary) = parse_burp_xml(xml.as_bytes()).expect("parse ok");
        assert_eq!(summary.items_seen, 2);
        assert_eq!(summary.items_imported, 1);
        assert_eq!(summary.items_skipped, 1);
        assert_eq!(flows.len(), 1);
        assert!(!summary.errors.is_empty());
    }
}
