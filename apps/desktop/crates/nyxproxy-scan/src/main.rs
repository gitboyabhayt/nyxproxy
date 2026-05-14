//! `nyxproxy-scan` — headless DAST runner used by the NyxProxy GitHub Action
//! (`gitboyabhayt/nyxproxy-action@v1`) and by anyone who wants to run a
//! NyxProxy security scan from CI / cron without launching the desktop app.
//!
//! It accepts a seed URL (or an OpenAPI/Swagger document — future extension),
//! crawls it with the [`nyxproxy_core::spider`], passively scans every visited
//! response with [`nyxproxy_core::scanner`], and exits with an appropriate
//! status code so a CI pipeline can fail the build on findings.
//!
//! Outputs:
//! * `--output-json <path>` — full machine-readable report.
//! * `--output-sarif <path>` — SARIF 2.1.0 file for GitHub code scanning.
//! * `--output-html <path>` — human-readable HTML report.
//! * always: a Markdown summary to stdout.
//!
//! Exit codes:
//! * `0` — no findings above the configured threshold.
//! * `1` — at least one finding above `--fail-on`.
//! * `2` — an internal error.

use std::collections::HashMap;
use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Context, Result};
use nyxproxy_core::history::HistoryStore;
use nyxproxy_core::model::{CapturedRequest, CapturedResponse, HeaderEntry, HttpFlow};
use nyxproxy_core::scanner::{self, Issue, IssueSeverity};
use nyxproxy_core::spider::{crawl, SpiderConfig, SpiderHit};
use serde::{Deserialize, Serialize};

#[derive(Debug)]
struct Args {
    target: String,
    scope: Vec<String>,
    fail_on: IssueSeverity,
    max_urls: usize,
    max_depth: u32,
    concurrency: usize,
    insecure: bool,
    output_json: Option<PathBuf>,
    output_sarif: Option<PathBuf>,
    output_html: Option<PathBuf>,
}

impl Args {
    fn parse() -> Result<Self> {
        let mut target: Option<String> = None;
        let mut scope: Vec<String> = Vec::new();
        let mut fail_on = IssueSeverity::High;
        let mut max_urls: usize = 200;
        let mut max_depth: u32 = 3;
        let mut concurrency: usize = 4;
        let mut insecure = false;
        let mut output_json: Option<PathBuf> = None;
        let mut output_sarif: Option<PathBuf> = None;
        let mut output_html: Option<PathBuf> = None;

        let mut argv = std::env::args().skip(1).peekable();
        while let Some(arg) = argv.next() {
            match arg.as_str() {
                "--target" => target = argv.next(),
                "--scope" => {
                    if let Some(v) = argv.next() {
                        scope.extend(v.split(',').map(str::trim).map(str::to_string));
                    }
                }
                "--fail-on" => {
                    fail_on = match argv.next().as_deref() {
                        Some("info") => IssueSeverity::Info,
                        Some("low") => IssueSeverity::Low,
                        Some("medium") => IssueSeverity::Medium,
                        Some("high") => IssueSeverity::High,
                        Some("critical") => IssueSeverity::Critical,
                        other => anyhow::bail!("invalid --fail-on: {other:?}"),
                    };
                }
                "--max-urls" => max_urls = argv.next().unwrap_or_default().parse().unwrap_or(200),
                "--max-depth" => max_depth = argv.next().unwrap_or_default().parse().unwrap_or(3),
                "--concurrency" => concurrency = argv.next().unwrap_or_default().parse().unwrap_or(4),
                "--insecure" => insecure = true,
                "--output-json" => output_json = argv.next().map(PathBuf::from),
                "--output-sarif" => output_sarif = argv.next().map(PathBuf::from),
                "--output-html" => output_html = argv.next().map(PathBuf::from),
                "-h" | "--help" => {
                    print_help();
                    std::process::exit(0);
                }
                "-V" | "--version" => {
                    println!("nyxproxy-scan {}", env!("CARGO_PKG_VERSION"));
                    std::process::exit(0);
                }
                other => anyhow::bail!("unknown argument: {other}"),
            }
        }

        let target = target.context("--target <url> is required")?;
        if scope.is_empty() {
            // Default scope = host portion of target.
            if let Ok(u) = url::Url::parse(&target) {
                if let Some(h) = u.host_str() {
                    scope.push(h.to_string());
                }
            }
        }

        Ok(Self {
            target,
            scope,
            fail_on,
            max_urls,
            max_depth,
            concurrency,
            insecure,
            output_json,
            output_sarif,
            output_html,
        })
    }
}

fn print_help() {
    eprintln!(
        "nyxproxy-scan {VERSION}

Headless NyxProxy security scanner. Spiders --target, passively scans every
response, and exits non-zero if findings above --fail-on are produced.

USAGE:
    nyxproxy-scan --target <URL> [OPTIONS]

OPTIONS:
    --target <URL>            Seed URL to start crawling (required).
    --scope <CSV>             Comma-separated host substrings allowed
                              (default: host of --target).
    --fail-on <SEVERITY>      Exit non-zero on this severity or higher.
                              info|low|medium|high|critical
                              (default: high)
    --max-urls <N>            Cap total URLs visited (default: 200).
    --max-depth <N>           Cap crawl depth from seed (default: 3).
    --concurrency <N>         Parallel requests (default: 4).
    --insecure                Ignore TLS certificate errors.
    --output-json <PATH>      Write full report as JSON.
    --output-sarif <PATH>     Write SARIF 2.1.0 file (GitHub code scanning).
    --output-html <PATH>      Write human-readable HTML report.
    -h, --help                Show this help.
    -V, --version             Show version.

EXIT CODES:
    0  No findings above --fail-on.
    1  At least one finding above --fail-on.
    2  Internal error.

EXAMPLES:
    nyxproxy-scan --target https://staging.example.com --fail-on medium \\
        --output-sarif report.sarif --output-html report.html
",
        VERSION = env!("CARGO_PKG_VERSION"),
    );
}

#[derive(Debug, Serialize, Deserialize)]
struct Report {
    schema: &'static str,
    nyxproxy_version: String,
    started_at: chrono::DateTime<chrono::Utc>,
    finished_at: chrono::DateTime<chrono::Utc>,
    target: String,
    urls_visited: usize,
    findings: Vec<Issue>,
    fail_on: IssueSeverity,
    failed: bool,
}

#[tokio::main]
async fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")))
        .init();

    let args = match Args::parse() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("nyxproxy-scan: {e:#}");
            return ExitCode::from(2);
        }
    };

    let started_at = chrono::Utc::now();
    let cfg = SpiderConfig {
        seed_url: args.target.clone(),
        scope_hosts: args.scope.clone(),
        max_depth: args.max_depth,
        max_urls: args.max_urls,
        concurrency: args.concurrency,
        follow_robots: true,
        insecure: args.insecure,
    };
    let hits: Vec<SpiderHit> = crawl(cfg, None).await;

    // Convert hits into synthetic flows so the passive scanner can run.
    let history = HistoryStore::new();
    for hit in &hits {
        if let Some(flow) = synthetic_flow_from_hit(hit) {
            history.insert(flow);
        }
    }

    let entries = history.list();
    let mut findings: Vec<Issue> = Vec::new();
    for entry in &entries {
        findings.extend(scanner::scan(&entry.flow));
    }

    let max_sev = findings
        .iter()
        .map(|i| i.severity)
        .max_by_key(|s| severity_rank(*s));
    let failed = matches!(
        max_sev,
        Some(sev) if severity_rank(sev) >= severity_rank(args.fail_on)
    );

    let finished_at = chrono::Utc::now();
    let report = Report {
        schema: "nyxproxy-scan-report/v1",
        nyxproxy_version: env!("CARGO_PKG_VERSION").to_string(),
        started_at,
        finished_at,
        target: args.target.clone(),
        urls_visited: hits.len(),
        findings: findings.clone(),
        fail_on: args.fail_on,
        failed,
    };

    if let Some(path) = &args.output_json {
        if let Err(e) = std::fs::write(path, serde_json::to_string_pretty(&report).unwrap()) {
            eprintln!("nyxproxy-scan: failed to write {}: {e}", path.display());
            return ExitCode::from(2);
        }
    }
    if let Some(path) = &args.output_sarif {
        let sarif = build_sarif(&report);
        if let Err(e) = std::fs::write(path, serde_json::to_string_pretty(&sarif).unwrap()) {
            eprintln!("nyxproxy-scan: failed to write {}: {e}", path.display());
            return ExitCode::from(2);
        }
    }
    if let Some(path) = &args.output_html {
        if let Err(e) = std::fs::write(path, render_html(&report)) {
            eprintln!("nyxproxy-scan: failed to write {}: {e}", path.display());
            return ExitCode::from(2);
        }
    }

    print_summary(&report);

    if failed {
        ExitCode::from(1)
    } else {
        ExitCode::from(0)
    }
}

fn synthetic_flow_from_hit(hit: &SpiderHit) -> Option<HttpFlow> {
    let url = url::Url::parse(&hit.url).ok()?;
    let scheme = url.scheme().to_string();
    let authority = url.host_str()?.to_string();
    let path = if let Some(q) = url.query() {
        format!("{}?{}", url.path(), q)
    } else {
        url.path().to_string()
    };
    let req = CapturedRequest {
        method: "GET".into(),
        url: hit.url.clone(),
        scheme,
        authority,
        path,
        http_version: "HTTP/1.1".into(),
        headers: Vec::new(),
        body_b64: String::new(),
        body_size: 0,
    };
    let mut flow = HttpFlow::new(req);
    flow.started_at = chrono::Utc::now();
    flow.tags.push("source:spider".into());
    if let Some(status) = hit.status {
        let mut headers: Vec<HeaderEntry> = Vec::new();
        if let Some(ct) = &hit.content_type {
            headers.push(HeaderEntry::new("content-type", ct));
        }
        flow.response = Some(CapturedResponse {
            status,
            http_version: "HTTP/1.1".into(),
            reason: String::new(),
            headers,
            body_b64: String::new(),
            body_size: hit.bytes.unwrap_or(0),
            elapsed_ms: hit.elapsed_ms,
        });
    } else if let Some(err) = &hit.error {
        flow.error = Some(err.clone());
    }
    Some(flow)
}

fn severity_rank(s: IssueSeverity) -> u32 {
    match s {
        IssueSeverity::Info => 0,
        IssueSeverity::Low => 1,
        IssueSeverity::Medium => 2,
        IssueSeverity::High => 3,
        IssueSeverity::Critical => 4,
    }
}

fn severity_str(s: IssueSeverity) -> &'static str {
    match s {
        IssueSeverity::Info => "note",
        IssueSeverity::Low => "note",
        IssueSeverity::Medium => "warning",
        IssueSeverity::High => "error",
        IssueSeverity::Critical => "error",
    }
}

fn build_sarif(report: &Report) -> serde_json::Value {
    use serde_json::json;
    let mut rules: HashMap<String, serde_json::Value> = HashMap::new();
    for issue in &report.findings {
        rules.entry(issue.rule_id.clone()).or_insert_with(|| {
            json!({
                "id": issue.rule_id,
                "name": issue.name,
                "shortDescription": { "text": issue.name },
                "fullDescription": { "text": issue.description },
                "defaultConfiguration": { "level": severity_str(issue.severity) },
                "helpUri": "https://github.com/gitboyabhayt/nyxproxy/blob/main/docs/features/scanner.md"
            })
        });
    }
    let results: Vec<serde_json::Value> = report
        .findings
        .iter()
        .map(|i| {
            json!({
                "ruleId": i.rule_id,
                "level": severity_str(i.severity),
                "message": { "text": i.description },
                "locations": [{
                    "physicalLocation": {
                        "artifactLocation": { "uri": format!("{}{}", i.host, i.path) }
                    }
                }],
                "properties": {
                    "severity": format!("{:?}", i.severity).to_lowercase(),
                    "confidence": format!("{:?}", i.confidence).to_lowercase(),
                    "evidence": i.evidence,
                }
            })
        })
        .collect();
    json!({
        "$schema": "https://raw.githubusercontent.com/oasis-tcs/sarif-spec/main/Schemata/sarif-schema-2.1.0.json",
        "version": "2.1.0",
        "runs": [{
            "tool": {
                "driver": {
                    "name": "nyxproxy-scan",
                    "version": report.nyxproxy_version,
                    "informationUri": "https://github.com/gitboyabhayt/nyxproxy",
                    "rules": rules.into_values().collect::<Vec<_>>()
                }
            },
            "results": results,
            "properties": {
                "target": report.target,
                "urlsVisited": report.urls_visited,
                "failOn": format!("{:?}", report.fail_on).to_lowercase(),
                "failed": report.failed,
                "startedAt": report.started_at.to_rfc3339(),
                "finishedAt": report.finished_at.to_rfc3339(),
            }
        }]
    })
}

fn render_html(report: &Report) -> String {
    let mut html = String::new();
    html.push_str("<!doctype html><html><head><meta charset=\"utf-8\"><title>NyxProxy scan report</title>");
    html.push_str("<style>body{font-family:sans-serif;max-width:960px;margin:32px auto;padding:0 16px;}h1{margin-bottom:0;}table{width:100%;border-collapse:collapse;}td,th{padding:6px 8px;border-bottom:1px solid #ddd;text-align:left;font-size:14px;}.sev-critical{background:#7a0000;color:white;padding:2px 6px;border-radius:4px;}.sev-high{background:#c0392b;color:white;padding:2px 6px;border-radius:4px;}.sev-medium{background:#d68910;color:white;padding:2px 6px;border-radius:4px;}.sev-low{background:#5dade2;color:white;padding:2px 6px;border-radius:4px;}.sev-info{background:#aab7b8;color:white;padding:2px 6px;border-radius:4px;}</style>");
    html.push_str("</head><body>");
    html.push_str(&format!(
        "<h1>NyxProxy scan report</h1><p>Target: <code>{}</code></p>",
        report.target
    ));
    html.push_str(&format!(
        "<p>URLs visited: <b>{}</b> · Findings: <b>{}</b> · Fail threshold: <b>{:?}</b> · Status: <b>{}</b></p>",
        report.urls_visited,
        report.findings.len(),
        report.fail_on,
        if report.failed { "FAILED" } else { "PASS" }
    ));
    html.push_str("<table><thead><tr><th>Severity</th><th>Rule</th><th>Name</th><th>Host</th><th>Path</th><th>Evidence</th></tr></thead><tbody>");
    for i in &report.findings {
        let sev = format!("{:?}", i.severity).to_lowercase();
        html.push_str(&format!(
            "<tr><td><span class=\"sev-{sev}\">{sev}</span></td><td><code>{}</code></td><td>{}</td><td>{}</td><td><code>{}</code></td><td>{}</td></tr>",
            i.rule_id,
            html_escape(&i.name),
            html_escape(&i.host),
            html_escape(&i.path),
            html_escape(i.evidence.as_deref().unwrap_or("")),
        ));
    }
    html.push_str("</tbody></table></body></html>");
    html
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

fn print_summary(report: &Report) {
    let mut buckets: HashMap<&'static str, usize> = HashMap::new();
    for i in &report.findings {
        *buckets.entry(match i.severity {
            IssueSeverity::Critical => "critical",
            IssueSeverity::High => "high",
            IssueSeverity::Medium => "medium",
            IssueSeverity::Low => "low",
            IssueSeverity::Info => "info",
        }).or_insert(0) += 1;
    }
    println!("## NyxProxy scan report");
    println!();
    println!("- Target: `{}`", report.target);
    println!("- URLs visited: **{}**", report.urls_visited);
    println!(
        "- Findings: critical=**{}** high=**{}** medium=**{}** low=**{}** info=**{}**",
        buckets.get("critical").copied().unwrap_or(0),
        buckets.get("high").copied().unwrap_or(0),
        buckets.get("medium").copied().unwrap_or(0),
        buckets.get("low").copied().unwrap_or(0),
        buckets.get("info").copied().unwrap_or(0),
    );
    println!(
        "- Status: **{}** (fail-on={:?})",
        if report.failed { "FAILED" } else { "PASS" },
        report.fail_on
    );
    if !report.findings.is_empty() {
        println!();
        println!("| Severity | Rule | Host | Path |");
        println!("|----------|------|------|------|");
        for i in &report.findings {
            println!(
                "| {:?} | `{}` | {} | `{}` |",
                i.severity, i.rule_id, i.host, i.path
            );
        }
    }
}
