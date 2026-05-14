//! End-to-end integration tests for the NyxProxy proxy core.
//!
//! Boots a tiny axum-free echo server on a random port, points a configured
//! [`Proxy`] at it, and verifies that traffic flowing through the proxy is
//! captured into history.

use std::net::SocketAddr;
use std::sync::Arc;

use bytes::Bytes;
use http_body_util::Full;
use hyper::body::Incoming;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use nyxproxy_core::ca::CertAuthority;
use nyxproxy_core::history::HistoryStore;
use nyxproxy_core::proxy::{Proxy, ProxyConfig};
use tokio::net::TcpListener;

async fn echo_response(req: Request<Incoming>) -> Result<Response<Full<Bytes>>, std::convert::Infallible> {
    let path = req.uri().path().to_string();
    let body = format!("echo path={path}");
    let mut resp = Response::new(Full::new(Bytes::from(body)));
    *resp.status_mut() = StatusCode::OK;
    Ok(resp)
}

async fn start_echo_server() -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        loop {
            let (stream, _) = match listener.accept().await {
                Ok(v) => v,
                Err(_) => continue,
            };
            tokio::spawn(async move {
                let io = TokioIo::new(stream);
                let _ = http1::Builder::new()
                    .keep_alive(false)
                    .serve_connection(io, service_fn(echo_response))
                    .await;
            });
        }
    });
    addr
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn proxies_plain_http_through_to_upstream_and_captures_history() {
    let upstream = start_echo_server().await;

    let ca = CertAuthority::ephemeral().unwrap();
    let history = HistoryStore::new();
    let config = ProxyConfig {
        listen_addr: "127.0.0.1:0".into(),
        intercept_enabled: false,
        scope_include: Vec::new(),
        scope_exclude: Vec::new(),
    };
    let proxy = Proxy::new(ca, history.clone(), config);
    let handle = proxy.bind().await.unwrap();
    let proxy_addr = handle.local_addr;

    // Build a reqwest client that sends through the proxy.
    let client = reqwest::Client::builder()
        .proxy(reqwest::Proxy::all(format!("http://{proxy_addr}")).unwrap())
        .build()
        .unwrap();

    let url = format!("http://{upstream}/hello");
    let resp = client.get(&url).send().await.unwrap();
    assert_eq!(resp.status().as_u16(), 200);
    let body = resp.text().await.unwrap();
    assert!(body.contains("path=/hello"), "body was {body:?}");

    // Allow the proxy task to flush the flow into history.
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    let list = history.list();
    assert!(!list.is_empty(), "no flows captured");
    let first = &list[0];
    assert_eq!(first.flow.request.method, "GET");
    assert!(first.flow.request.url.contains("/hello"));
    assert_eq!(first.flow.response.as_ref().unwrap().status, 200);

    handle.shutdown();
    let _ = Arc::new(handle).clone();
}

/// Negotiate HTTP/2 through the MITM CONNECT tunnel and verify ALPN.
///
/// This proves that:
///  * the proxy advertises `h2` via ALPN on the leaf certificate it mints,
///  * a client that prefers `h2` ends up speaking HTTP/2 inside the tunnel,
///  * the `http2::Builder` path of [`Proxy::mitm_tunnel`] is reachable.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn mitm_negotiates_http2_via_alpn() {
    use rustls::pki_types::ServerName;
    use rustls::RootCertStore;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let ca = CertAuthority::ephemeral().unwrap();
    let ca_cert_pem = ca.cert_pem().to_string();

    let history = HistoryStore::new();
    let config = ProxyConfig {
        listen_addr: "127.0.0.1:0".into(),
        intercept_enabled: false,
        scope_include: Vec::new(),
        scope_exclude: Vec::new(),
    };
    let proxy = Proxy::new(ca, history.clone(), config);
    let handle = proxy.bind().await.unwrap();
    let proxy_addr = handle.local_addr;

    // 1) Open a raw TCP connection to the proxy.
    let mut stream = tokio::net::TcpStream::connect(proxy_addr).await.unwrap();

    // 2) Send a CONNECT request for an arbitrary upstream — we never actually
    //    forward, we only care about the inner TLS handshake.
    stream
        .write_all(b"CONNECT example.com:443 HTTP/1.1\r\nHost: example.com:443\r\n\r\n")
        .await
        .unwrap();

    // 3) Read CONNECT response — should be HTTP/1.1 200.
    let mut buf = [0u8; 1024];
    let n = stream.read(&mut buf).await.unwrap();
    let head = std::str::from_utf8(&buf[..n]).unwrap();
    assert!(head.contains("200"), "CONNECT response was: {head:?}");

    // 4) Build a client TLS config that trusts the NyxProxy CA and advertises
    //    `h2` first, then `http/1.1`.
    let mut roots = RootCertStore::empty();
    for cert in rustls_pemfile::certs(&mut ca_cert_pem.as_bytes()) {
        roots.add(cert.unwrap()).unwrap();
    }
    let mut tls_cfg = rustls::ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();
    tls_cfg.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];

    let connector = tokio_rustls::TlsConnector::from(Arc::new(tls_cfg));
    let server_name = ServerName::try_from("example.com").unwrap();
    let tls = connector.connect(server_name, stream).await.unwrap();

    // 5) Inspect the negotiated ALPN protocol.
    let (_io, client_conn) = tls.get_ref();
    let alpn = client_conn.alpn_protocol().map(|b| b.to_vec());
    assert_eq!(
        alpn.as_deref(),
        Some(&b"h2"[..]),
        "client and proxy must negotiate HTTP/2 via ALPN, got {alpn:?}"
    );

    handle.shutdown();
}

/// Fall back to HTTP/1.1 when the client doesn't advertise `h2`.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn mitm_falls_back_to_http1_when_client_offers_h1_only() {
    use rustls::pki_types::ServerName;
    use rustls::RootCertStore;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let ca = CertAuthority::ephemeral().unwrap();
    let ca_cert_pem = ca.cert_pem().to_string();

    let history = HistoryStore::new();
    let config = ProxyConfig {
        listen_addr: "127.0.0.1:0".into(),
        ..Default::default()
    };
    let proxy = Proxy::new(ca, history, config);
    let handle = proxy.bind().await.unwrap();
    let proxy_addr = handle.local_addr;

    let mut stream = tokio::net::TcpStream::connect(proxy_addr).await.unwrap();
    stream
        .write_all(b"CONNECT example.com:443 HTTP/1.1\r\nHost: example.com:443\r\n\r\n")
        .await
        .unwrap();
    let mut buf = [0u8; 1024];
    let _ = stream.read(&mut buf).await.unwrap();

    let mut roots = RootCertStore::empty();
    for cert in rustls_pemfile::certs(&mut ca_cert_pem.as_bytes()) {
        roots.add(cert.unwrap()).unwrap();
    }
    let mut tls_cfg = rustls::ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();
    tls_cfg.alpn_protocols = vec![b"http/1.1".to_vec()];

    let connector = tokio_rustls::TlsConnector::from(Arc::new(tls_cfg));
    let server_name = ServerName::try_from("example.com").unwrap();
    let tls = connector.connect(server_name, stream).await.unwrap();

    let (_io, client_conn) = tls.get_ref();
    let alpn = client_conn.alpn_protocol().map(|b| b.to_vec());
    assert_eq!(
        alpn.as_deref(),
        Some(&b"http/1.1"[..]),
        "fallback ALPN should be http/1.1, got {alpn:?}"
    );

    handle.shutdown();
}
