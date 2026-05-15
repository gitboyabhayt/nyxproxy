//! NyxProxy distributed-scan worker.
//!
//! Long-polls a NyxProxy backend's `/scan/jobs/next` endpoint, executes the
//! requested HTTP request, runs the passive scanner on the captured flow,
//! and posts the result back. Designed to scale linearly — start one process
//! per worker slot (`NYX_WORKER_ID=worker-1`, etc.) on any machine that can
//! reach both the backend and the targets.
//!
//! Configuration:
//!
//! * `NYX_BACKEND_URL`  — required, e.g. `https://nyxproxy-backend.onrender.com`
//! * `NYX_BACKEND_TOKEN` — optional bearer token (if the backend is locked
//!   down with `BACKEND_API_TOKEN`).
//! * `NYX_WORKER_ID`    — defaults to a random `worker-<8 hex>` UUID slice.
//! * `NYX_POLL_TIMEOUT` — long-poll seconds for `/scan/jobs/next` (default 25).

use std::collections::HashMap;
use std::env;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use base64::Engine;
use chrono::Utc;
use nyxproxy_core::model::{
    CapturedRequest, CapturedResponse, HeaderEntry, HttpFlow,
};
use nyxproxy_core::scanner::{scan, Issue};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TargetSpec {
    url: String,
    method: String,
    headers: HashMap<String, String>,
    body_b64: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct ScanJob {
    id: String,
    target: TargetSpec,
    rules: Vec<String>,
    label: Option<String>,
}

#[derive(Debug, Serialize)]
struct ScanResultPayload {
    findings: Vec<Issue>,
    error: Option<String>,
    elapsed_ms: u64,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("nyxproxy_worker=info")),
        )
        .init();

    let backend = env::var("NYX_BACKEND_URL").context("NYX_BACKEND_URL is required")?;
    let token = env::var("NYX_BACKEND_TOKEN").ok();
    let worker_id = env::var("NYX_WORKER_ID")
        .unwrap_or_else(|_| format!("worker-{}", &Uuid::new_v4().simple().to_string()[..8]));
    let poll_timeout: u64 = env::var("NYX_POLL_TIMEOUT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(25);

    let http = reqwest::Client::builder()
        .timeout(Duration::from_secs(60))
        .build()
        .context("build reqwest client")?;

    tracing::info!(
        target: "nyxproxy_worker",
        worker_id = %worker_id,
        backend = %backend,
        "worker online, awaiting jobs"
    );

    let mut backoff = Duration::from_millis(500);
    loop {
        match next_job(&http, &backend, token.as_deref(), &worker_id, poll_timeout).await {
            Ok(Some(job)) => {
                backoff = Duration::from_millis(500);
                let job_id = job.id.clone();
                let label = job.label.clone().unwrap_or_default();
                tracing::info!(
                    target: "nyxproxy_worker",
                    job_id = %job_id,
                    label = %label,
                    "claimed job"
                );
                let payload = execute(&http, &job).await;
                if let Err(err) =
                    submit(&http, &backend, token.as_deref(), &job_id, &worker_id, &payload).await
                {
                    tracing::error!(
                        target: "nyxproxy_worker",
                        job_id = %job_id,
                        ?err,
                        "failed to submit result"
                    );
                }
            }
            Ok(None) => {
                // long-poll returned no work — loop immediately.
            }
            Err(err) => {
                tracing::warn!(
                    target: "nyxproxy_worker",
                    ?err,
                    backoff_ms = backoff.as_millis() as u64,
                    "poll error, backing off"
                );
                tokio::time::sleep(backoff).await;
                backoff = (backoff * 2).min(Duration::from_secs(15));
            }
        }
    }
}

async fn next_job(
    client: &reqwest::Client,
    backend: &str,
    token: Option<&str>,
    worker_id: &str,
    timeout_secs: u64,
) -> Result<Option<ScanJob>> {
    let url = format!(
        "{}/scan/jobs/next?worker_id={}&wait={}",
        backend.trim_end_matches('/'),
        urlencoding::encode_str(worker_id),
        timeout_secs
    );
    let mut req = client.get(&url);
    if let Some(t) = token {
        req = req.bearer_auth(t);
    }
    let res = req.send().await?;
    if res.status() == 204 {
        return Ok(None);
    }
    if !res.status().is_success() {
        anyhow::bail!("poll failed: HTTP {}", res.status());
    }
    let job: ScanJob = res.json().await?;
    Ok(Some(job))
}

async fn submit(
    client: &reqwest::Client,
    backend: &str,
    token: Option<&str>,
    job_id: &str,
    worker_id: &str,
    payload: &ScanResultPayload,
) -> Result<()> {
    let url = format!(
        "{}/scan/jobs/{}/result?worker_id={}",
        backend.trim_end_matches('/'),
        job_id,
        urlencoding::encode_str(worker_id),
    );
    let mut req = client.post(&url).json(payload);
    if let Some(t) = token {
        req = req.bearer_auth(t);
    }
    let res = req.send().await?;
    if !res.status().is_success() {
        anyhow::bail!("submit failed: HTTP {}", res.status());
    }
    Ok(())
}

async fn execute(client: &reqwest::Client, job: &ScanJob) -> ScanResultPayload {
    let started = Instant::now();
    match execute_inner(client, job).await {
        Ok(flow) => {
            let mut findings = scan(&flow);
            if !job.rules.is_empty() {
                findings.retain(|f| job.rules.contains(&f.rule_id));
            }
            ScanResultPayload {
                findings,
                error: None,
                elapsed_ms: started.elapsed().as_millis() as u64,
            }
        }
        Err(err) => ScanResultPayload {
            findings: Vec::new(),
            error: Some(format!("{err:#}")),
            elapsed_ms: started.elapsed().as_millis() as u64,
        },
    }
}

async fn execute_inner(client: &reqwest::Client, job: &ScanJob) -> Result<HttpFlow> {
    let body = match &job.target.body_b64 {
        Some(b) if !b.is_empty() => base64::engine::general_purpose::STANDARD.decode(b)?,
        _ => Vec::new(),
    };
    let method = reqwest::Method::from_bytes(job.target.method.as_bytes())
        .context("invalid HTTP method")?;
    let mut req = client.request(method.clone(), &job.target.url).body(body.clone());
    for (k, v) in &job.target.headers {
        req = req.header(k, v);
    }
    let started = Instant::now();
    let resp = req.send().await?;
    let elapsed_ms = started.elapsed().as_millis() as u64;
    let status = resp.status().as_u16();
    let version = format!("{:?}", resp.version());
    let reason = resp
        .status()
        .canonical_reason()
        .unwrap_or("")
        .to_string();
    let resp_headers: Vec<HeaderEntry> = resp
        .headers()
        .iter()
        .map(|(k, v)| {
            HeaderEntry::new(k.to_string(), v.to_str().unwrap_or("").to_string())
        })
        .collect();
    let resp_body = resp.bytes().await?;

    let url = url::Url::parse(&job.target.url)?;
    let request = CapturedRequest {
        method: job.target.method.clone(),
        url: job.target.url.clone(),
        scheme: url.scheme().to_string(),
        authority: url.host_str().unwrap_or("").to_string(),
        path: url.path().to_string(),
        http_version: "HTTP/1.1".into(),
        headers: job
            .target
            .headers
            .iter()
            .map(|(k, v)| HeaderEntry::new(k, v))
            .collect(),
        body_b64: base64::engine::general_purpose::STANDARD.encode(&body),
        body_size: body.len(),
    };
    let response = CapturedResponse {
        status,
        http_version: version,
        reason,
        headers: resp_headers,
        body_b64: base64::engine::general_purpose::STANDARD.encode(&resp_body),
        body_size: resp_body.len(),
        elapsed_ms,
    };

    let mut flow = HttpFlow::new(request);
    flow.response = Some(response);
    flow.started_at = Utc::now();
    Ok(flow)
}

// Tiny inline urlencoding to avoid adding a dependency.
mod urlencoding {
    pub fn encode_str(s: &str) -> String {
        let mut out = String::with_capacity(s.len());
        for byte in s.bytes() {
            match byte {
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                    out.push(byte as char)
                }
                _ => out.push_str(&format!("%{byte:02X}")),
            }
        }
        out
    }
}
