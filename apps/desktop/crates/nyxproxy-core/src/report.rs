//! Report exporter — render captured history + scanner issues into a
//! self-contained HTML or JSON report.
//!
//! The HTML output is intentionally dependency-free: a single document
//! containing inline CSS, statistics, a flow table, and an issues table.
//! Useful for sharing the result of an engagement without requiring the
//! reader to install NyxProxy.

use base64::Engine;
use serde::{Deserialize, Serialize};

use crate::history::HistoryEntry;
use crate::scanner::Issue;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Report {
    pub generated_at: String,
    pub flow_count: usize,
    pub issue_count: usize,
    pub by_severity: std::collections::HashMap<String, usize>,
    pub flows: Vec<HistoryEntry>,
    pub issues: Vec<Issue>,
}

pub fn build(history: &[HistoryEntry], issues: &[Issue]) -> Report {
    let mut by_severity = std::collections::HashMap::new();
    for issue in issues {
        let key = severity_label(&issue.severity).to_string();
        *by_severity.entry(key).or_insert(0usize) += 1;
    }
    Report {
        generated_at: chrono::Utc::now().to_rfc3339(),
        flow_count: history.len(),
        issue_count: issues.len(),
        by_severity,
        flows: history.to_vec(),
        issues: issues.to_vec(),
    }
}

pub fn render_json(report: &Report) -> String {
    serde_json::to_string_pretty(report).unwrap_or_else(|_| "{}".to_string())
}

pub fn render_html(report: &Report) -> String {
    let mut out = String::new();
    out.push_str(HTML_PREAMBLE);
    out.push_str(&format!(
        "<h1>NyxProxy Report</h1><p>Generated at <strong>{}</strong></p>",
        html_escape(&report.generated_at)
    ));
    out.push_str(&format!(
        "<div class=\"stats\">\
           <div class=\"stat\"><span class=\"num\">{}</span><span>flows</span></div>\
           <div class=\"stat\"><span class=\"num\">{}</span><span>issues</span></div>",
        report.flow_count, report.issue_count
    ));
    for (sev, count) in &report.by_severity {
        out.push_str(&format!(
            "<div class=\"stat sev-{0}\"><span class=\"num\">{1}</span><span>{0}</span></div>",
            html_escape(sev),
            count
        ));
    }
    out.push_str("</div>");

    if !report.issues.is_empty() {
        out.push_str("<h2>Issues</h2><table><thead><tr><th>Severity</th><th>Confidence</th><th>Issue</th><th>Host</th><th>Path</th><th>Evidence</th></tr></thead><tbody>");
        for issue in &report.issues {
            out.push_str(&format!(
                "<tr><td><span class=\"sev sev-{}\">{}</span></td><td>{}</td><td><strong>{}</strong><br/><small>{}</small></td><td>{}</td><td><code>{}</code></td><td><code>{}</code></td></tr>",
                severity_label(&issue.severity),
                severity_label(&issue.severity),
                confidence_label(&issue.confidence),
                html_escape(&issue.name),
                html_escape(&issue.description),
                html_escape(&issue.host),
                html_escape(&issue.path),
                html_escape(issue.evidence.as_deref().unwrap_or(""))
            ));
        }
        out.push_str("</tbody></table>");
    }

    out.push_str("<h2>HTTP history</h2><table><thead><tr><th>#</th><th>Method</th><th>Host</th><th>Path</th><th>Status</th><th>Notes</th></tr></thead><tbody>");
    for (i, entry) in report.flows.iter().enumerate().take(500) {
        out.push_str(&format!(
            "<tr><td>{}</td><td>{}</td><td>{}</td><td><code>{}</code></td><td>{}</td><td>{}</td></tr>",
            i + 1,
            html_escape(&entry.flow.request.method),
            html_escape(&entry.flow.request.authority),
            html_escape(&entry.flow.request.path),
            entry
                .flow
                .response
                .as_ref()
                .map(|r| r.status.to_string())
                .unwrap_or_else(|| "—".into()),
            html_escape(entry.note.as_deref().unwrap_or(""))
        ));
    }
    out.push_str("</tbody></table>");
    if report.flows.len() > 500 {
        out.push_str(&format!(
            "<p><em>{} more flows truncated. Export JSON for the full record.</em></p>",
            report.flows.len() - 500
        ));
    }
    out.push_str(HTML_EPILOGUE);
    out
}

fn severity_label(sev: &crate::scanner::IssueSeverity) -> &'static str {
    use crate::scanner::IssueSeverity::*;
    match sev {
        Info => "info",
        Low => "low",
        Medium => "medium",
        High => "high",
        Critical => "critical",
    }
}

fn confidence_label(conf: &crate::scanner::IssueConfidence) -> &'static str {
    use crate::scanner::IssueConfidence::*;
    match conf {
        Tentative => "tentative",
        Firm => "firm",
        Certain => "certain",
    }
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Return the JSON payload base64-encoded as a `data:` URL so it can be
/// embedded in an HTML download link without server round-trips.
pub fn json_data_url(report: &Report) -> String {
    let json = render_json(report);
    let b64 = base64::engine::general_purpose::STANDARD.encode(json.as_bytes());
    format!("data:application/json;base64,{b64}")
}

const HTML_PREAMBLE: &str = r#"<!doctype html>
<html lang="en"><head><meta charset="utf-8"/>
<title>NyxProxy Report</title>
<style>
:root { color-scheme: dark; }
body { background:#0c0c0e; color:#eaeaea; font-family:-apple-system,Segoe UI,Inter,Helvetica,Arial,sans-serif; margin:0; padding:24px; }
h1 { margin:0 0 4px; font-size:22px; }
h2 { margin:24px 0 8px; font-size:16px; color:#ffcd5b; }
.stats { display:flex; gap:8px; flex-wrap:wrap; margin:12px 0 4px; }
.stat { background:#171719; border:1px solid #2a2a2c; border-radius:6px; padding:8px 14px; display:flex; flex-direction:column; }
.stat .num { font-size:22px; font-weight:600; }
.stat span:last-child { font-size:11px; opacity:.6; letter-spacing:.5px; text-transform:uppercase; }
.stat.sev-critical { border-color:#a4202a; }
.stat.sev-high { border-color:#d35f5f; }
.stat.sev-medium { border-color:#d6a55b; }
.stat.sev-low { border-color:#7297c4; }
.stat.sev-info { border-color:#5f6b75; }
table { width:100%; border-collapse:collapse; margin:8px 0 16px; font-size:13px; }
th, td { text-align:left; padding:6px 10px; border-bottom:1px solid #1d1d1f; vertical-align:top; }
th { background:#16161a; font-weight:600; text-transform:uppercase; font-size:11px; letter-spacing:.5px; color:#9a9aa3; }
code { font-family:ui-monospace,Menlo,Consolas,monospace; font-size:12px; color:#b3c5db; }
.sev { display:inline-block; padding:1px 8px; border-radius:10px; font-size:11px; text-transform:uppercase; letter-spacing:.5px; }
.sev-critical { background:#5b1a23; color:#ffd0d6; }
.sev-high { background:#5b2620; color:#ffc8b0; }
.sev-medium { background:#4d3f17; color:#ffe3a5; }
.sev-low { background:#1f3251; color:#bdd6ff; }
.sev-info { background:#262a30; color:#cfcfd8; }
</style></head><body>"#;

const HTML_EPILOGUE: &str = "</body></html>";

#[cfg(test)]
mod tests {
    use super::*;
    use crate::history::{HistoryEntry, HistoryStore};
    use crate::model::{CapturedRequest, HeaderEntry, HttpFlow};
    use crate::scanner::{Issue, IssueConfidence, IssueSeverity};
    use base64::Engine;

    fn sample_entry() -> HistoryEntry {
        let req = CapturedRequest {
            method: "GET".into(),
            url: "https://example.com/".into(),
            scheme: "https".into(),
            authority: "example.com".into(),
            path: "/".into(),
            http_version: "HTTP/1.1".into(),
            headers: vec![HeaderEntry::new("Host", "example.com")],
            body_b64: base64::engine::general_purpose::STANDARD.encode(b""),
            body_size: 0,
        };
        let store = HistoryStore::new();
        store.insert(HttpFlow::new(req));
        store.list().pop().unwrap()
    }

    #[test]
    fn report_aggregates_counts() {
        let issues = vec![Issue {
            id: "r|f|h|p".into(),
            flow_id: "f".into(),
            rule_id: "r".into(),
            name: "n".into(),
            severity: IssueSeverity::High,
            confidence: IssueConfidence::Firm,
            description: "d".into(),
            evidence: Some("e".into()),
            remediation: None,
            host: "example.com".into(),
            path: "/".into(),
        }];
        let report = build(&[sample_entry()], &issues);
        assert_eq!(report.flow_count, 1);
        assert_eq!(report.issue_count, 1);
        assert_eq!(report.by_severity.get("high"), Some(&1));
    }

    #[test]
    fn html_render_includes_table_headers() {
        let report = build(&[sample_entry()], &[]);
        let html = render_html(&report);
        assert!(html.contains("<title>NyxProxy Report</title>"));
        assert!(html.contains("HTTP history"));
        assert!(html.contains("example.com"));
    }

    #[test]
    fn json_data_url_decodes_back_to_report() {
        let report = build(&[sample_entry()], &[]);
        let url = json_data_url(&report);
        let prefix = "data:application/json;base64,";
        let b64 = url.strip_prefix(prefix).expect("data url prefix");
        let bytes = base64::engine::general_purpose::STANDARD.decode(b64).unwrap();
        let parsed: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(parsed["flow_count"], 1);
    }
}
