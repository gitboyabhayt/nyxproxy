//! Local HTTP bridge — small `127.0.0.1:<port>` server used by the
//! [NyxProxy browser extension](../../../../extensions/browser/README.md) and
//! the CI/CD action to push requests into NyxProxy from outside the desktop
//! app.
//!
//! Endpoints:
//!
//! - `OPTIONS *` — CORS preflight (browser extensions need this).
//! - `GET /api/v1/ping` — returns `{ "ok": true, "version": "..." }`.
//! - `POST /api/v1/import-url` — body: `{ "url": "https://...", "tags": [..] }`.
//!   The bridge fetches the URL with `reqwest` and inserts the
//!   request/response pair into history tagged with `source:browser-ext`.
//! - `POST /api/v1/import-flow` — body: `HttpFlow` JSON. Inserts the flow
//!   verbatim (used by power users who already have a serialised flow).
//!
//! The bridge is intentionally tiny — only `Content-Type: application/json`
//! is accepted, all responses are JSON, and the listener binds to
//! `127.0.0.1` so it is unreachable from other hosts on the LAN.

use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::Arc;

use base64::Engine;
use http_body_util::{BodyExt, Full};
use hyper::body::{Bytes, Incoming};
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Method, Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use serde::{Deserialize, Serialize};
use tokio::net::TcpListener;
use tracing::{debug, info, warn};

use crate::error::{NyxError, NyxResult};
use crate::history::HistoryStore;
use crate::model::{CapturedRequest, CapturedResponse, HeaderEntry, HttpFlow};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BridgeConfig {
    pub listen_addr: String,
}

impl Default for BridgeConfig {
    fn default() -> Self {
        Self {
            listen_addr: "127.0.0.1:8090".into(),
        }
    }
}

pub struct BridgeHandle {
    pub local_addr: SocketAddr,
    pub task: tokio::task::JoinHandle<()>,
}

#[derive(Debug, Deserialize)]
struct ImportUrlBody {
    url: String,
    #[serde(default)]
    method: Option<String>,
    #[serde(default)]
    tags: Vec<String>,
}

#[derive(Debug, Serialize)]
struct ApiOk<T: Serialize> {
    ok: bool,
    data: T,
}

#[derive(Debug, Serialize)]
struct ApiErr {
    ok: bool,
    error: String,
}

/// Bind the bridge listener and spawn a serve loop. The returned
/// [`BridgeHandle`] keeps the task alive — drop it to stop the bridge.
pub async fn start(cfg: BridgeConfig, history: HistoryStore) -> NyxResult<BridgeHandle> {
    let addr: SocketAddr = cfg
        .listen_addr
        .parse()
        .map_err(|e| NyxError::Proxy(format!("invalid bridge listen_addr: {e}")))?;
    let listener = TcpListener::bind(addr)
        .await
        .map_err(|e| NyxError::Proxy(format!("bridge bind failed: {e}")))?;
    let local_addr = listener.local_addr()?;
    info!(%local_addr, "nyxproxy bridge listening");

    let history = Arc::new(history);
    let task = tokio::spawn(async move {
        loop {
            let (stream, _peer) = match listener.accept().await {
                Ok(c) => c,
                Err(e) => {
                    warn!(?e, "bridge accept error");
                    continue;
                }
            };
            let history = history.clone();
            tokio::spawn(async move {
                let io = TokioIo::new(stream);
                let svc = service_fn(move |req| {
                    let history = history.clone();
                    async move { Ok::<_, Infallible>(handle(req, history).await) }
                });
                if let Err(err) = http1::Builder::new().serve_connection(io, svc).await {
                    debug!(?err, "bridge connection ended");
                }
            });
        }
    });

    Ok(BridgeHandle { local_addr, task })
}

async fn handle(req: Request<Incoming>, history: Arc<HistoryStore>) -> Response<Full<Bytes>> {
    let method = req.method().clone();
    let path = req.uri().path().to_string();

    // CORS preflight — browser extensions sometimes use fetch() with
    // application/json which triggers a preflight.
    if method == Method::OPTIONS {
        return cors(Response::builder()
            .status(StatusCode::NO_CONTENT)
            .body(Full::new(Bytes::new()))
            .unwrap());
    }

    match (method.clone(), path.as_str()) {
        (Method::GET, "/api/v1/ping") => json_ok(serde_json::json!({
            "version": env!("CARGO_PKG_VERSION"),
        })),
        (Method::POST, "/api/v1/import-url") => match read_json::<ImportUrlBody>(req).await {
            Ok(body) => import_url(body, &history).await,
            Err(e) => json_err(StatusCode::BAD_REQUEST, e),
        },
        (Method::POST, "/api/v1/import-flow") => match read_json::<HttpFlow>(req).await {
            Ok(flow) => {
                let mut flow = flow;
                if !flow.tags.iter().any(|t| t == "source:browser-ext") {
                    flow.tags.push("source:browser-ext".into());
                }
                history.insert(flow.clone());
                json_ok(serde_json::json!({ "flow_id": flow.id }))
            }
            Err(e) => json_err(StatusCode::BAD_REQUEST, e),
        },
        _ => json_err(StatusCode::NOT_FOUND, format!("no route for {method} {path}")),
    }
}

fn cors<B>(mut resp: Response<B>) -> Response<B> {
    let h = resp.headers_mut();
    h.insert(
        "access-control-allow-origin",
        hyper::header::HeaderValue::from_static("*"),
    );
    h.insert(
        "access-control-allow-methods",
        hyper::header::HeaderValue::from_static("GET, POST, OPTIONS"),
    );
    h.insert(
        "access-control-allow-headers",
        hyper::header::HeaderValue::from_static("content-type"),
    );
    resp
}

fn json_ok<T: Serialize>(data: T) -> Response<Full<Bytes>> {
    let body = serde_json::to_vec(&ApiOk { ok: true, data }).unwrap_or_default();
    cors(Response::builder()
        .status(StatusCode::OK)
        .header("content-type", "application/json")
        .body(Full::new(Bytes::from(body)))
        .unwrap())
}

fn json_err(status: StatusCode, msg: impl Into<String>) -> Response<Full<Bytes>> {
    let body = serde_json::to_vec(&ApiErr {
        ok: false,
        error: msg.into(),
    })
    .unwrap_or_default();
    cors(Response::builder()
        .status(status)
        .header("content-type", "application/json")
        .body(Full::new(Bytes::from(body)))
        .unwrap())
}

async fn read_json<T: for<'de> Deserialize<'de>>(req: Request<Incoming>) -> Result<T, String> {
    let limit = 4 * 1024 * 1024; // 4 MiB
    let body = req
        .into_body()
        .collect()
        .await
        .map_err(|e| format!("read body: {e}"))?
        .to_bytes();
    if body.len() > limit {
        return Err(format!("body too large ({} bytes, limit {})", body.len(), limit));
    }
    serde_json::from_slice::<T>(&body).map_err(|e| format!("invalid JSON: {e}"))
}

async fn import_url(body: ImportUrlBody, history: &HistoryStore) -> Response<Full<Bytes>> {
    let url = body.url.clone();
    let method = body.method.unwrap_or_else(|| "GET".into()).to_uppercase();
    let parsed = match url::Url::parse(&url) {
        Ok(u) => u,
        Err(e) => return json_err(StatusCode::BAD_REQUEST, format!("invalid url: {e}")),
    };
    let scheme = parsed.scheme().to_string();
    let authority = match parsed.host_str() {
        Some(h) => h.to_string(),
        None => return json_err(StatusCode::BAD_REQUEST, "url missing host"),
    };
    let path = if let Some(q) = parsed.query() {
        format!("{}?{}", parsed.path(), q)
    } else {
        parsed.path().to_string()
    };

    let client = reqwest::Client::builder()
        .user_agent("NyxProxy-Bridge/0.1")
        .build()
        .expect("client build");
    let req_method = match reqwest::Method::from_bytes(method.as_bytes()) {
        Ok(m) => m,
        Err(_) => return json_err(StatusCode::BAD_REQUEST, "unsupported HTTP method"),
    };
    let started = std::time::Instant::now();
    let resp = match client.request(req_method, &url).send().await {
        Ok(r) => r,
        Err(e) => return json_err(StatusCode::BAD_GATEWAY, format!("upstream fetch failed: {e}")),
    };
    let status = resp.status().as_u16();
    let version = format!("{:?}", resp.version());
    let resp_headers: Vec<HeaderEntry> = resp
        .headers()
        .iter()
        .map(|(k, v)| HeaderEntry::new(k.as_str(), v.to_str().unwrap_or("")))
        .collect();
    let resp_bytes = match resp.bytes().await {
        Ok(b) => b,
        Err(e) => return json_err(StatusCode::BAD_GATEWAY, format!("upstream read failed: {e}")),
    };
    let elapsed_ms = started.elapsed().as_millis() as u64;

    let req_struct = CapturedRequest {
        method: method.clone(),
        url: url.clone(),
        scheme,
        authority,
        path,
        http_version: "HTTP/1.1".into(),
        headers: Vec::new(),
        body_b64: String::new(),
        body_size: 0,
    };
    let mut flow = HttpFlow::new(req_struct);
    let mut tags = body.tags.clone();
    tags.push("source:browser-ext".into());
    flow.tags = tags;
    flow.response = Some(CapturedResponse {
        status,
        http_version: version,
        reason: String::new(),
        headers: resp_headers,
        body_b64: base64::engine::general_purpose::STANDARD.encode(&resp_bytes),
        body_size: resp_bytes.len(),
        elapsed_ms,
    });
    let flow_id = flow.id.clone();
    history.insert(flow);
    json_ok(serde_json::json!({ "flow_id": flow_id, "status": status }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn ping_returns_version() {
        let history = HistoryStore::new();
        let handle = start(
            BridgeConfig {
                listen_addr: "127.0.0.1:0".into(),
            },
            history,
        )
        .await
        .unwrap();
        let url = format!("http://{}/api/v1/ping", handle.local_addr);
        let body = reqwest::get(&url).await.unwrap().text().await.unwrap();
        assert!(body.contains("\"ok\":true"));
        assert!(body.contains("\"version\""));
        handle.task.abort();
    }

    #[tokio::test]
    async fn import_flow_inserts_into_history() {
        let history = HistoryStore::new();
        let handle = start(
            BridgeConfig {
                listen_addr: "127.0.0.1:0".into(),
            },
            history.clone(),
        )
        .await
        .unwrap();
        let req = CapturedRequest {
            method: "GET".into(),
            url: "https://example.com/x".into(),
            scheme: "https".into(),
            authority: "example.com".into(),
            path: "/x".into(),
            http_version: "HTTP/1.1".into(),
            headers: Vec::new(),
            body_b64: String::new(),
            body_size: 0,
        };
        let flow = HttpFlow::new(req);
        let url = format!("http://{}/api/v1/import-flow", handle.local_addr);
        let client = reqwest::Client::new();
        let resp = client.post(&url).json(&flow).send().await.unwrap();
        assert_eq!(resp.status(), 200);
        let entries = history.list();
        assert_eq!(entries.len(), 1);
        assert!(entries[0].flow.tags.contains(&"source:browser-ext".to_string()));
        handle.task.abort();
    }

    #[tokio::test]
    async fn invalid_route_returns_404_json() {
        let history = HistoryStore::new();
        let handle = start(
            BridgeConfig {
                listen_addr: "127.0.0.1:0".into(),
            },
            history,
        )
        .await
        .unwrap();
        let url = format!("http://{}/nope", handle.local_addr);
        let resp = reqwest::get(&url).await.unwrap();
        assert_eq!(resp.status(), 404);
        let body = resp.text().await.unwrap();
        assert!(body.contains("\"ok\":false"));
        handle.task.abort();
    }
}
