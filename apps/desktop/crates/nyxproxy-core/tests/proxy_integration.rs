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
