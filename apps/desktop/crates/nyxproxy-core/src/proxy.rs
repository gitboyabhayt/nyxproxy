//! The intercepting HTTPS proxy.
//!
//! Implementation outline:
//!
//! 1. Listen for plaintext HTTP on a configurable TCP port.
//! 2. Plain HTTP requests are proxied directly via [`reqwest`] and captured.
//! 3. HTTPS works via the standard CONNECT tunnel: clients send
//!    `CONNECT host:port HTTP/1.1`. We respond `200`, upgrade the connection,
//!    perform a server-side TLS handshake using a leaf certificate minted by
//!    [`crate::ca::CertAuthority`] (advertising `h2,http/1.1` via ALPN), and
//!    then serve HTTP/2 or HTTP/1.1 over the decrypted stream — capturing each
//!    inner request and forwarding it to the real target with `reqwest`.
//!
//! ALPN handling:
//!   * If the client negotiates `h2` we serve with [`hyper::server::conn::http2`].
//!   * Otherwise we serve with [`hyper::server::conn::http1`] (covers HTTP/1.1
//!     and WebSocket upgrades).
//!
//! HTTP/3 (QUIC) cannot be MITM'd through a standard CONNECT tunnel because
//! CONNECT tunnels TCP, not UDP. Browsers that advertise QUIC support via
//! `Alt-Svc` would bypass the proxy entirely — to prevent that we strip the
//! `Alt-Svc` and `Alternate-Service` response headers in [`forward_capture`].
//! Upstream HTTP/3 is available transparently when `reqwest` is built with the
//! `http3` feature.

use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use base64::Engine;
use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use hyper::body::Incoming;
use hyper::server::conn::{http1, http2};
use hyper::service::service_fn;
use hyper::{Method, Request, Response, StatusCode, Uri};
use hyper_util::rt::{TokioExecutor, TokioIo};
use parking_lot::RwLock;
use rustls::ServerConfig;
use serde::{Deserialize, Serialize};
use tokio::io::{copy_bidirectional, AsyncRead, AsyncWrite};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::broadcast;
use tokio_rustls::TlsAcceptor;
use tracing::{debug, error, info, warn};

use crate::ca::CertAuthority;
use crate::error::{NyxError, NyxResult};
use crate::history::HistoryStore;
use crate::intercept::{InterceptDecision, InterceptQueue};
use crate::model::{CapturedRequest, CapturedResponse, HeaderEntry, HttpFlow, ProxyEvent};
use crate::websocket::{self as ws, WsStore};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxyConfig {
    pub listen_addr: String,
    /// When false, every flow is forwarded without modification but is still
    /// captured into history. When true, flows are held in the intercept queue
    /// until the user explicitly forwards or drops them.
    #[serde(default)]
    pub intercept_enabled: bool,
    /// Inclusion list — if non-empty, only hosts matching one of these
    /// substrings are intercepted (others are tunnelled opaquely).
    #[serde(default)]
    pub scope_include: Vec<String>,
    /// Exclusion list — hosts matching any of these substrings bypass the
    /// MITM entirely.
    #[serde(default)]
    pub scope_exclude: Vec<String>,
}

impl Default for ProxyConfig {
    fn default() -> Self {
        Self {
            listen_addr: "127.0.0.1:8089".into(),
            intercept_enabled: false,
            scope_include: Vec::new(),
            scope_exclude: vec![
                "translate.googleapis.com".into(),
                "clients2.google.com".into(),
                "safebrowsing.googleapis.com".into(),
            ],
        }
    }
}

#[derive(Clone)]
pub struct Proxy {
    pub ca: CertAuthority,
    pub history: HistoryStore,
    pub config: Arc<RwLock<ProxyConfig>>,
    pub events: broadcast::Sender<ProxyEvent>,
    pub intercept: InterceptQueue,
    pub ws_store: WsStore,
}

impl Proxy {
    pub fn new(ca: CertAuthority, history: HistoryStore, config: ProxyConfig) -> Self {
        let (events, _) = broadcast::channel(1024);
        Self {
            ca,
            history,
            config: Arc::new(RwLock::new(config)),
            events,
            intercept: InterceptQueue::new(),
            ws_store: WsStore::new(),
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<ProxyEvent> {
        self.events.subscribe()
    }

    pub fn update_config(&self, config: ProxyConfig) {
        *self.config.write() = config;
    }

    pub fn snapshot_config(&self) -> ProxyConfig {
        self.config.read().clone()
    }

    /// Bind a listening socket. Returns the bound address (useful when port 0
    /// was requested) and a [`ProxyHandle`] that drives the accept loop.
    pub async fn bind(self) -> NyxResult<ProxyHandle> {
        let addr = self
            .config
            .read()
            .listen_addr
            .parse::<SocketAddr>()
            .map_err(|e| NyxError::Proxy(format!("invalid listen_addr: {e}")))?;
        let listener = TcpListener::bind(addr)
            .await
            .map_err(|e| NyxError::Proxy(format!("bind {addr}: {e}")))?;
        let local = listener.local_addr()?;
        let _ = self.events.send(ProxyEvent::Started {
            listen_addr: local.to_string(),
        });
        info!(%local, "nyxproxy listening");

        let shutdown = tokio::sync::Notify::new();
        let handle = ProxyHandle {
            local_addr: local,
            shutdown: Arc::new(shutdown),
            join: None,
        };
        let shutdown_for_loop = handle.shutdown.clone();
        let proxy = self.clone();
        let join = tokio::spawn(async move {
            proxy.accept_loop(listener, shutdown_for_loop).await;
        });
        let mut handle = handle;
        handle.join = Some(join);
        Ok(handle)
    }

    async fn accept_loop(self, listener: TcpListener, shutdown: Arc<tokio::sync::Notify>) {
        loop {
            tokio::select! {
                _ = shutdown.notified() => {
                    info!("proxy shutting down");
                    let _ = self.events.send(ProxyEvent::Stopped);
                    return;
                }
                accepted = listener.accept() => {
                    match accepted {
                        Ok((stream, peer)) => {
                            debug!(%peer, "accepted client");
                            let proxy = self.clone();
                            tokio::spawn(async move {
                                if let Err(err) = proxy.handle_client(stream).await {
                                    warn!(?err, "client handler returned error");
                                    let _ = proxy.events.send(ProxyEvent::Error { message: err.to_string() });
                                }
                            });
                        }
                        Err(err) => {
                            warn!(?err, "accept failed");
                        }
                    }
                }
            }
        }
    }

    async fn handle_client(&self, stream: TcpStream) -> NyxResult<()> {
        let io = TokioIo::new(stream);
        let service = service_fn({
            let proxy = self.clone();
            move |req: Request<Incoming>| {
                let proxy = proxy.clone();
                async move { proxy.dispatch(req).await }
            }
        });

        let result = http1::Builder::new()
            .preserve_header_case(true)
            .keep_alive(true)
            .serve_connection(io, service)
            .with_upgrades()
            .await;
        if let Err(err) = result {
            debug!(?err, "client connection closed");
        }
        Ok(())
    }

    async fn dispatch(
        &self,
        req: Request<Incoming>,
    ) -> Result<Response<Full<Bytes>>, Infallible> {
        if req.method() == Method::CONNECT {
            return Ok(self.handle_connect(req).await);
        }
        Ok(self.handle_plain(req).await)
    }

    async fn handle_plain(&self, req: Request<Incoming>) -> Response<Full<Bytes>> {
        match plain_request(req).await {
            Ok((captured, body)) => match forward_capture(self.clone(), captured, body).await {
                Ok(response) => response,
                Err(err) => error_response(StatusCode::BAD_GATEWAY, &err.to_string()),
            },
            Err(err) => error_response(StatusCode::BAD_REQUEST, &err.to_string()),
        }
    }

    async fn handle_connect(&self, req: Request<Incoming>) -> Response<Full<Bytes>> {
        let authority = match req.uri().authority().cloned() {
            Some(a) => a,
            None => {
                return error_response(StatusCode::BAD_REQUEST, "CONNECT without authority");
            }
        };
        let host = authority.host().to_string();
        let port = authority.port_u16().unwrap_or(443);

        // Tunnel opaquely if the host is excluded from scope.
        let cfg = self.snapshot_config();
        if should_tunnel_opaquely(&host, &cfg) {
            let proxy = self.clone();
            tokio::spawn(async move {
                if let Err(err) = proxy.tunnel_opaque(req, &host, port).await {
                    debug!(?err, host, port, "opaque tunnel error");
                }
            });
            return ok_connect_response();
        }

        let server_config = match build_server_config(&self.ca, &host) {
            Ok(c) => c,
            Err(err) => {
                error!(?err, "tls server config failed");
                return error_response(StatusCode::INTERNAL_SERVER_ERROR, &err.to_string());
            }
        };

        let proxy = self.clone();
        tokio::spawn(async move {
            if let Err(err) = proxy.mitm_tunnel(req, server_config, host.clone(), port).await {
                warn!(?err, host, port, "MITM tunnel error");
            }
        });
        ok_connect_response()
    }

    async fn tunnel_opaque(
        &self,
        req: Request<Incoming>,
        host: &str,
        port: u16,
    ) -> NyxResult<()> {
        let mut upgraded = hyper::upgrade::on(req)
            .await
            .map_err(|e| NyxError::Proxy(format!("upgrade: {e}")))?;
        let mut upstream = TcpStream::connect((host, port))
            .await
            .map_err(|e| NyxError::Upstream(format!("opaque dial {host}:{port}: {e}")))?;
        let mut upgraded_io = TokioIo::new(&mut upgraded);
        copy_bidirectional(&mut upgraded_io, &mut upstream)
            .await
            .map_err(|e| NyxError::Proxy(format!("opaque relay: {e}")))?;
        Ok(())
    }

    async fn mitm_tunnel(
        &self,
        req: Request<Incoming>,
        server_config: Arc<ServerConfig>,
        host: String,
        port: u16,
    ) -> NyxResult<()> {
        let upgraded = hyper::upgrade::on(req)
            .await
            .map_err(|e| NyxError::Proxy(format!("upgrade: {e}")))?;

        let acceptor = TlsAcceptor::from(server_config);
        let tls_stream = acceptor
            .accept(TokioIo::new(upgraded))
            .await
            .map_err(|e| NyxError::Tls(format!("accept: {e}")))?;
        let alpn = tls_stream
            .get_ref()
            .1
            .alpn_protocol()
            .map(|p| p.to_vec());
        let io = TokioIo::new(tls_stream);

        let service = service_fn({
            let proxy = self.clone();
            let host = host.clone();
            move |req: Request<Incoming>| {
                let proxy = proxy.clone();
                let host = host.clone();
                async move { proxy.serve_intercepted(req, host, port).await }
            }
        });

        match alpn.as_deref() {
            Some(b"h2") => {
                debug!(host, port, "MITM serving HTTP/2");
                http2::Builder::new(TokioExecutor::new())
                    .serve_connection(io, service)
                    .await
                    .map_err(|e| NyxError::Http(format!("inner h2 serve: {e}")))?;
            }
            _ => {
                debug!(host, port, "MITM serving HTTP/1.1");
                http1::Builder::new()
                    .preserve_header_case(true)
                    .keep_alive(true)
                    .serve_connection(io, service)
                    .with_upgrades()
                    .await
                    .map_err(|e| NyxError::Http(format!("inner h1 serve: {e}")))?;
            }
        }
        Ok(())
    }

    async fn serve_intercepted(
        &self,
        req: Request<Incoming>,
        host: String,
        port: u16,
    ) -> Result<Response<Full<Bytes>>, Infallible> {
        if is_websocket_upgrade(&req) {
            return Ok(self.serve_websocket_upgrade(req, host, port).await);
        }
        match intercepted_request(req, &host, port).await {
            Ok((captured, body)) => match forward_capture(self.clone(), captured, body).await {
                Ok(resp) => Ok(resp),
                Err(err) => Ok(error_response(StatusCode::BAD_GATEWAY, &err.to_string())),
            },
            Err(err) => Ok(error_response(StatusCode::BAD_REQUEST, &err.to_string())),
        }
    }

    /// Handle a `GET ...\r\nUpgrade: websocket` request reaching the MITM.
    ///
    /// We open a raw TLS connection to the upstream, send the same handshake,
    /// wait for the 101 response, then reply 101 back to the client. Both
    /// sides upgrade and we run [`crate::websocket::proxy_pump`] between
    /// them, capturing every frame into [`WsStore`].
    async fn serve_websocket_upgrade(
        &self,
        req: Request<Incoming>,
        host: String,
        port: u16,
    ) -> Response<Full<Bytes>> {
        let path_and_query = req
            .uri()
            .path_and_query()
            .map(|p| p.as_str().to_string())
            .unwrap_or_else(|| "/".into());
        let url = if port == 443 {
            format!("wss://{host}{path_and_query}")
        } else {
            format!("wss://{host}:{port}{path_and_query}")
        };

        // Compute the accept token from the client's Sec-WebSocket-Key.
        let key = match req
            .headers()
            .get("sec-websocket-key")
            .and_then(|v| v.to_str().ok())
        {
            Some(k) => k.to_string(),
            None => {
                return error_response(
                    StatusCode::BAD_REQUEST,
                    "Sec-WebSocket-Key header missing",
                );
            }
        };
        let accept = sec_websocket_accept(&key);

        // Capture handshake headers into the request store so the user can
        // inspect what was actually sent.
        let mut handshake_lines: Vec<u8> = Vec::with_capacity(256);
        handshake_lines.extend_from_slice(
            format!("GET {path_and_query} HTTP/1.1\r\n").as_bytes(),
        );
        let mut have_host = false;
        for (name, value) in req.headers().iter() {
            let lower = name.as_str().to_ascii_lowercase();
            if lower == "host" {
                have_host = true;
            }
            if matches!(
                lower.as_str(),
                "proxy-connection" | "proxy-authorization"
            ) {
                continue;
            }
            if let Ok(v) = value.to_str() {
                handshake_lines.extend_from_slice(name.as_str().as_bytes());
                handshake_lines.extend_from_slice(b": ");
                handshake_lines.extend_from_slice(v.as_bytes());
                handshake_lines.extend_from_slice(b"\r\n");
            }
        }
        if !have_host {
            let header = if port == 443 {
                format!("Host: {host}\r\n")
            } else {
                format!("Host: {host}:{port}\r\n")
            };
            handshake_lines.extend_from_slice(header.as_bytes());
        }
        handshake_lines.extend_from_slice(b"\r\n");

        let upgrade_fut = hyper::upgrade::on(req);

        // Spawn the upstream bridge so we can return the 101 immediately.
        let proxy = self.clone();
        let host_for_task = host.clone();
        let url_for_task = url.clone();
        tokio::spawn(async move {
            if let Err(err) = proxy
                .bridge_websocket(
                    upgrade_fut,
                    host_for_task,
                    port,
                    handshake_lines,
                    url_for_task,
                )
                .await
            {
                debug!(?err, "websocket bridge ended");
            }
        });

        Response::builder()
            .status(StatusCode::SWITCHING_PROTOCOLS)
            .header("Upgrade", "websocket")
            .header("Connection", "Upgrade")
            .header("Sec-WebSocket-Accept", accept)
            .body(Full::new(Bytes::new()))
            .unwrap_or_else(|_| error_response(StatusCode::INTERNAL_SERVER_ERROR, "build 101"))
    }

    async fn bridge_websocket(
        &self,
        upgrade_fut: hyper::upgrade::OnUpgrade,
        host: String,
        port: u16,
        handshake_bytes: Vec<u8>,
        url: String,
    ) -> NyxResult<()> {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio_rustls::rustls::pki_types::ServerName;

        // 1) Connect TCP + TLS to upstream.
        let tcp = TcpStream::connect((host.as_str(), port))
            .await
            .map_err(|e| NyxError::Upstream(format!("ws dial {host}:{port}: {e}")))?;

        let root_store = root_cert_store();
        let client_cfg = tokio_rustls::rustls::ClientConfig::builder()
            .with_root_certificates(root_store)
            .with_no_client_auth();
        let connector = tokio_rustls::TlsConnector::from(Arc::new(client_cfg));
        let server_name = ServerName::try_from(host.clone())
            .map_err(|e| NyxError::Tls(format!("server name: {e}")))?;
        let mut upstream = connector
            .connect(server_name, tcp)
            .await
            .map_err(|e| NyxError::Tls(format!("ws connect: {e}")))?;

        // 2) Send handshake and read response headers.
        upstream
            .write_all(&handshake_bytes)
            .await
            .map_err(|e| NyxError::Upstream(format!("ws handshake write: {e}")))?;

        let mut response_header = Vec::with_capacity(512);
        let mut buf = [0u8; 256];
        loop {
            let n = upstream
                .read(&mut buf)
                .await
                .map_err(|e| NyxError::Upstream(format!("ws handshake read: {e}")))?;
            if n == 0 {
                return Err(NyxError::Upstream("ws upstream closed".into()));
            }
            response_header.extend_from_slice(&buf[..n]);
            if response_header.windows(4).any(|w| w == b"\r\n\r\n") {
                break;
            }
            if response_header.len() > 16_384 {
                return Err(NyxError::Upstream("ws handshake too large".into()));
            }
        }
        if !response_header.starts_with(b"HTTP/1.1 101")
            && !response_header.starts_with(b"HTTP/1.0 101")
        {
            return Err(NyxError::Upstream(format!(
                "ws upstream refused: {}",
                String::from_utf8_lossy(&response_header)
                    .lines()
                    .next()
                    .unwrap_or("")
            )));
        }
        // Trim handshake bytes (anything past \r\n\r\n is the first frame).
        let split = response_header
            .windows(4)
            .position(|w| w == b"\r\n\r\n")
            .map(|i| i + 4)
            .unwrap_or(response_header.len());
        let extra_after_handshake = response_header.split_off(split);

        // 3) Await the client-side upgrade.
        let upgraded_client = upgrade_fut
            .await
            .map_err(|e| NyxError::Proxy(format!("client upgrade: {e}")))?;
        let mut client_io = TokioIo::new(upgraded_client);

        // If upstream already sent bytes past its handshake (rare), flush
        // them to the client before entering the pump loop.
        if !extra_after_handshake.is_empty() {
            client_io
                .write_all(&extra_after_handshake)
                .await
                .map_err(|e| NyxError::Proxy(format!("write extra: {e}")))?;
        }

        // 4) Register a WS session and pump frames.
        let (session, replay_rx) = self.ws_store.start_session(url, host);
        ws::proxy_pump(
            self.ws_store.clone(),
            session,
            replay_rx,
            client_io,
            upstream,
        )
        .await
    }
}

fn is_websocket_upgrade(req: &Request<Incoming>) -> bool {
    let upgrade = req
        .headers()
        .get("upgrade")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.eq_ignore_ascii_case("websocket"))
        .unwrap_or(false);
    let connection = req
        .headers()
        .get("connection")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_ascii_lowercase().contains("upgrade"))
        .unwrap_or(false);
    upgrade && connection
}

fn sec_websocket_accept(key: &str) -> String {
    use sha1::{Digest, Sha1};
    let mut hasher = Sha1::new();
    hasher.update(key.as_bytes());
    hasher.update(b"258EAFA5-E914-47DA-95CA-C5AB0DC85B11");
    let digest = hasher.finalize();
    base64::engine::general_purpose::STANDARD.encode(digest)
}

fn root_cert_store() -> Arc<tokio_rustls::rustls::RootCertStore> {
    use once_cell::sync::OnceCell;
    static STORE: OnceCell<Arc<tokio_rustls::rustls::RootCertStore>> = OnceCell::new();
    STORE
        .get_or_init(|| {
            let mut s = tokio_rustls::rustls::RootCertStore::empty();
            s.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
            Arc::new(s)
        })
        .clone()
}

pub struct ProxyHandle {
    pub local_addr: SocketAddr,
    shutdown: Arc<tokio::sync::Notify>,
    join: Option<tokio::task::JoinHandle<()>>,
}

impl ProxyHandle {
    pub fn shutdown(&self) {
        self.shutdown.notify_waiters();
    }

    pub async fn join(self) {
        if let Some(h) = self.join {
            let _ = h.await;
        }
    }
}

fn should_tunnel_opaquely(host: &str, cfg: &ProxyConfig) -> bool {
    if !cfg.scope_include.is_empty() {
        let included = cfg.scope_include.iter().any(|s| host.contains(s.as_str()));
        if !included {
            return true;
        }
    }
    cfg.scope_exclude.iter().any(|s| host.contains(s.as_str()))
}

fn build_server_config(ca: &CertAuthority, host: &str) -> NyxResult<Arc<ServerConfig>> {
    let (chain, key) = ca.leaf_for(host)?;
    let key_clone = (*key).clone_key();
    let mut cfg = ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(chain, key_clone)
        .map_err(|e| NyxError::Tls(e.to_string()))?;
    // Advertise HTTP/2 + HTTP/1.1 via ALPN so the client can negotiate the
    // best protocol it supports. WebSocket upgrades still require HTTP/1.1, so
    // we keep `http/1.1` in the list.
    cfg.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];
    Ok(Arc::new(cfg))
}

fn ok_connect_response() -> Response<Full<Bytes>> {
    let mut resp = Response::new(Full::new(Bytes::new()));
    *resp.status_mut() = StatusCode::OK;
    resp
}

fn error_response(status: StatusCode, message: &str) -> Response<Full<Bytes>> {
    let mut resp = Response::new(Full::new(Bytes::from(message.as_bytes().to_vec())));
    *resp.status_mut() = status;
    resp
}

async fn plain_request(req: Request<Incoming>) -> NyxResult<(CapturedRequest, Vec<u8>)> {
    let (parts, body) = req.into_parts();
    let collected = body
        .collect()
        .await
        .map_err(|e| NyxError::Http(format!("read body: {e}")))?;
    let bytes = collected.to_bytes().to_vec();
    let uri = parts.uri.clone();
    let scheme = uri
        .scheme_str()
        .unwrap_or("http")
        .to_string();
    let authority = uri.authority().map(|a| a.to_string()).unwrap_or_default();
    let url = uri.to_string();
    let captured = CapturedRequest {
        method: parts.method.to_string(),
        url,
        scheme,
        authority,
        path: uri.path_and_query().map(|p| p.to_string()).unwrap_or_else(|| "/".into()),
        http_version: format!("{:?}", parts.version),
        headers: header_entries(&parts.headers),
        body_b64: base64::engine::general_purpose::STANDARD.encode(&bytes),
        body_size: bytes.len(),
    };
    Ok((captured, bytes))
}

async fn intercepted_request(
    req: Request<Incoming>,
    host: &str,
    port: u16,
) -> NyxResult<(CapturedRequest, Vec<u8>)> {
    let (mut parts, body) = req.into_parts();
    let collected = body
        .collect()
        .await
        .map_err(|e| NyxError::Http(format!("read body: {e}")))?;
    let bytes = collected.to_bytes().to_vec();

    let path_and_query = parts
        .uri
        .path_and_query()
        .map(|p| p.as_str().to_string())
        .unwrap_or_else(|| "/".into());

    let absolute = if port == 443 {
        format!("https://{host}{path_and_query}")
    } else {
        format!("https://{host}:{port}{path_and_query}")
    };
    parts.uri = Uri::try_from(absolute.as_str())
        .map_err(|e| NyxError::BadRequest(format!("uri: {e}")))?;

    let captured = CapturedRequest {
        method: parts.method.to_string(),
        url: absolute,
        scheme: "https".into(),
        authority: if port == 443 {
            host.to_string()
        } else {
            format!("{host}:{port}")
        },
        path: path_and_query,
        http_version: format!("{:?}", parts.version),
        headers: header_entries(&parts.headers),
        body_b64: base64::engine::general_purpose::STANDARD.encode(&bytes),
        body_size: bytes.len(),
    };
    Ok((captured, bytes))
}

fn header_entries(headers: &hyper::HeaderMap) -> Vec<HeaderEntry> {
    headers
        .iter()
        .filter_map(|(name, value)| {
            value
                .to_str()
                .ok()
                .map(|v| HeaderEntry::new(name.as_str(), v))
        })
        .collect()
}

async fn forward_capture(
    proxy: Proxy,
    captured: CapturedRequest,
    body: Vec<u8>,
) -> NyxResult<Response<Full<Bytes>>> {
    let intercept_enabled = proxy.config.read().intercept_enabled;
    let (captured, body) = if intercept_enabled {
        match proxy.intercept.enqueue(captured, body).await {
            InterceptDecision::Forward { request, body } => (request, body),
            InterceptDecision::Drop => {
                return Ok(error_response(
                    StatusCode::GATEWAY_TIMEOUT,
                    "request dropped by NyxProxy intercept",
                ));
            }
        }
    } else {
        (captured, body)
    };

    let mut flow = HttpFlow::new(captured.clone());
    flow.tags.push("proxy".into());

    let response = match upstream_call(&captured, &body).await {
        Ok(r) => r,
        Err(err) => {
            flow.error = Some(err.to_string());
            proxy.history.insert(flow.clone());
            let _ = proxy.events.send(ProxyEvent::Flow { flow });
            return Err(err);
        }
    };
    flow.response = Some(response.clone());
    proxy.history.insert(flow.clone());
    let _ = proxy.events.send(ProxyEvent::Flow { flow });

    let mut resp_builder = Response::builder().status(response.status);
    for header in &response.headers {
        // Skip hop-by-hop headers Hyper will refuse to relay.
        let lower = header.name.to_ascii_lowercase();
        if matches!(
            lower.as_str(),
            "connection"
                | "transfer-encoding"
                | "content-encoding"
                | "content-length"
                | "keep-alive"
                | "proxy-authenticate"
                | "proxy-authorization"
                | "te"
                | "trailers"
                | "upgrade"
                | "alt-svc"
                | "alternate-service"
        ) {
            continue;
        }
        resp_builder = resp_builder.header(header.name.clone(), header.value.clone());
    }
    let body_bytes = response.body_bytes();
    let resp = resp_builder
        .body(Full::new(Bytes::from(body_bytes)))
        .map_err(|e| NyxError::Http(format!("response build: {e}")))?;
    Ok(resp)
}

async fn upstream_call(
    captured: &CapturedRequest,
    body: &[u8],
) -> NyxResult<CapturedResponse> {
    let client = reqwest::Client::builder()
        .user_agent("NyxProxy/0.1")
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(60))
        .build()
        .map_err(|e| NyxError::Upstream(format!("build client: {e}")))?;
    let method = reqwest::Method::from_bytes(captured.method.as_bytes())
        .map_err(|e| NyxError::BadRequest(format!("invalid method: {e}")))?;
    let mut req = client.request(method, &captured.url);
    for header in &captured.headers {
        let lower = header.name.to_ascii_lowercase();
        if matches!(
            lower.as_str(),
            "host" | "content-length" | "connection" | "transfer-encoding" | "proxy-connection"
        ) {
            continue;
        }
        req = req.header(header.name.clone(), header.value.clone());
    }
    if !body.is_empty() {
        req = req.body(body.to_vec());
    }
    let start = Instant::now();
    let response = req
        .send()
        .await
        .map_err(|e| NyxError::Upstream(format!("send: {e}")))?;
    let status = response.status();
    let version = format!("{:?}", response.version());
    let mut headers: Vec<HeaderEntry> = Vec::with_capacity(response.headers().len());
    for (name, value) in response.headers().iter() {
        if let Ok(v) = value.to_str() {
            headers.push(HeaderEntry::new(name.as_str(), v));
        }
    }
    let bytes = response
        .bytes()
        .await
        .map_err(|e| NyxError::Upstream(format!("read body: {e}")))?;
    let elapsed_ms = start.elapsed().as_millis() as u64;
    Ok(CapturedResponse {
        status: status.as_u16(),
        http_version: version,
        reason: status.canonical_reason().unwrap_or("").to_string(),
        headers,
        body_size: bytes.len(),
        body_b64: base64::engine::general_purpose::STANDARD.encode(&bytes),
        elapsed_ms,
    })
}

#[allow(dead_code)]
async fn drain_io<S>(mut io: S) -> std::io::Result<()>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    tokio::io::AsyncWriteExt::shutdown(&mut io).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_default_excludes_translate() {
        let cfg = ProxyConfig::default();
        assert!(cfg.scope_exclude.iter().any(|s| s.contains("translate.googleapis.com")));
    }

    #[test]
    fn opaque_tunnel_when_host_in_exclude() {
        let cfg = ProxyConfig {
            scope_exclude: vec!["banned.example".into()],
            ..ProxyConfig::default()
        };
        assert!(should_tunnel_opaquely("api.banned.example", &cfg));
        assert!(!should_tunnel_opaquely("ok.example", &cfg));
    }

    #[test]
    fn scope_include_restricts_mitm() {
        let cfg = ProxyConfig {
            scope_include: vec!["target.example".into()],
            scope_exclude: vec![],
            ..ProxyConfig::default()
        };
        assert!(should_tunnel_opaquely("other.example", &cfg));
        assert!(!should_tunnel_opaquely("api.target.example", &cfg));
    }

    #[test]
    fn server_config_advertises_h2_and_h1_alpn() {
        // Build a CA + leaf for a dummy host and assert ALPN includes h2 then http/1.1.
        let ca = CertAuthority::ephemeral().expect("ephemeral CA");
        let cfg = build_server_config(&ca, "example.com").expect("build cfg");
        assert_eq!(
            cfg.alpn_protocols,
            vec![b"h2".to_vec(), b"http/1.1".to_vec()],
            "ALPN must advertise h2 first, then http/1.1"
        );
    }
}
