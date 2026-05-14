//! Minimal HTTP/3 (QUIC) upstream client.
//!
//! This module is intentionally small: it lets the proxy and the Repeater
//! issue an HTTP/3 request to an arbitrary URL using `quinn` + `h3`. It is
//! **not** wired into the MITM accept loop — Chrome and friends still talk
//! HTTP/1.1/HTTP/2 to NyxProxy via the standard CONNECT tunnel. The point of
//! this module is to give Repeater an "upgrade to HTTP/3" button that can
//! actually talk to a server's `:443/udp` endpoint and report what came back.
//!
//! Implementation notes:
//!
//! * Uses `rustls` (ring) with `webpki-roots` for cert verification so we
//!   don't pull in OS cert stores.
//! * Resolves the target host with `tokio::net::lookup_host` and picks the
//!   first matching IPv4/IPv6 address.
//! * Opens a single QUIC connection per call (no connection pooling yet — h3
//!   benefits less from pooling since it already multiplexes, but a real pool
//!   is on the roadmap).

use std::net::{SocketAddr, ToSocketAddrs};
use std::sync::Arc;
use std::time::Duration;

use bytes::{Buf, Bytes};
use h3::client;
use h3_quinn::quinn;
use http::{Method, Request, Uri};
use serde::{Deserialize, Serialize};

use crate::error::{NyxError, NyxResult};

/// Result of an HTTP/3 request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct H3Response {
    pub status: u16,
    pub http_version: String,
    pub headers: Vec<(String, String)>,
    pub body_b64: String,
    pub body_size: usize,
    pub elapsed_ms: u64,
}

fn rustls_client_config_for_h3() -> NyxResult<Arc<rustls::ClientConfig>> {
    let mut roots = rustls::RootCertStore::empty();
    for ta in webpki_roots::TLS_SERVER_ROOTS.iter() {
        roots.add(rustls::pki_types::CertificateDer::from(ta.subject_public_key_info.as_ref().to_vec())).ok();
    }
    let mut cfg = rustls::ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();
    cfg.alpn_protocols = vec![b"h3".to_vec()];
    cfg.enable_early_data = true;
    Ok(Arc::new(cfg))
}

/// Issue a single HTTP/3 request and return the response.
///
/// `body` is sent verbatim. `method` defaults to `GET` if empty.
pub async fn request(
    method: &str,
    url: &str,
    headers: &[(String, String)],
    body: &[u8],
) -> NyxResult<H3Response> {
    let uri: Uri = url
        .parse()
        .map_err(|e| NyxError::BadRequest(format!("invalid url: {e}")))?;
    let scheme = uri.scheme_str().unwrap_or("https");
    if scheme != "https" {
        return Err(NyxError::BadRequest("HTTP/3 requires https://".into()));
    }
    let host = uri
        .host()
        .ok_or_else(|| NyxError::BadRequest("missing host".into()))?
        .to_string();
    let port = uri.port_u16().unwrap_or(443);
    let authority = if uri.port_u16().is_some() {
        format!("{host}:{port}")
    } else {
        host.clone()
    };

    let addr: SocketAddr = (host.as_str(), port)
        .to_socket_addrs()
        .map_err(|e| NyxError::Upstream(format!("dns: {e}")))?
        .next()
        .ok_or_else(|| NyxError::Upstream("no DNS records".into()))?;

    let crypto = rustls_client_config_for_h3()?;
    let quic_client = quinn::crypto::rustls::QuicClientConfig::try_from(crypto.as_ref().clone())
        .map_err(|e| NyxError::Tls(format!("quic crypto: {e}")))?;
    let mut client_config = quinn::ClientConfig::new(Arc::new(quic_client));
    let mut transport = quinn::TransportConfig::default();
    transport.max_idle_timeout(Some(Duration::from_secs(15).try_into().unwrap()));
    client_config.transport_config(Arc::new(transport));

    let bind_addr: SocketAddr = if addr.is_ipv6() {
        "[::]:0".parse().unwrap()
    } else {
        "0.0.0.0:0".parse().unwrap()
    };
    let mut endpoint = quinn::Endpoint::client(bind_addr)
        .map_err(|e| NyxError::Upstream(format!("quic endpoint: {e}")))?;
    endpoint.set_default_client_config(client_config);

    let start = std::time::Instant::now();
    let conn = endpoint
        .connect(addr, host.as_str())
        .map_err(|e| NyxError::Upstream(format!("quic connect setup: {e}")))?
        .await
        .map_err(|e| NyxError::Upstream(format!("quic connect: {e}")))?;

    let (mut driver, mut send_request) = client::new(h3_quinn::Connection::new(conn))
        .await
        .map_err(|e| NyxError::Upstream(format!("h3 handshake: {e}")))?;

    let drive = tokio::spawn(async move {
        let _ = std::future::poll_fn(|cx| driver.poll_close(cx)).await;
    });

    let method = if method.is_empty() {
        Method::GET
    } else {
        Method::from_bytes(method.as_bytes())
            .map_err(|e| NyxError::BadRequest(format!("method: {e}")))?
    };

    let mut req_builder = Request::builder()
        .method(method)
        .uri(format!(
            "{}://{}{}",
            scheme,
            authority,
            uri.path_and_query()
                .map(|p| p.as_str())
                .unwrap_or("/")
        ));
    for (name, value) in headers {
        // h3 rejects connection-specific headers.
        let lower = name.to_ascii_lowercase();
        if matches!(
            lower.as_str(),
            "host"
                | "content-length"
                | "connection"
                | "transfer-encoding"
                | "proxy-connection"
                | "upgrade"
                | "keep-alive"
        ) {
            continue;
        }
        req_builder = req_builder.header(name, value);
    }
    let req = req_builder
        .body(())
        .map_err(|e| NyxError::Http(format!("request build: {e}")))?;

    let mut stream = send_request
        .send_request(req)
        .await
        .map_err(|e| NyxError::Upstream(format!("h3 send: {e}")))?;

    if !body.is_empty() {
        stream
            .send_data(Bytes::from(body.to_vec()))
            .await
            .map_err(|e| NyxError::Upstream(format!("h3 send_data: {e}")))?;
    }
    stream
        .finish()
        .await
        .map_err(|e| NyxError::Upstream(format!("h3 finish: {e}")))?;

    let resp = stream
        .recv_response()
        .await
        .map_err(|e| NyxError::Upstream(format!("h3 recv_response: {e}")))?;

    let status = resp.status().as_u16();
    let mut header_pairs: Vec<(String, String)> = Vec::with_capacity(resp.headers().len());
    for (name, value) in resp.headers() {
        if let Ok(v) = value.to_str() {
            header_pairs.push((name.as_str().to_string(), v.to_string()));
        }
    }

    let mut body_bytes: Vec<u8> = Vec::new();
    while let Some(chunk) = stream
        .recv_data()
        .await
        .map_err(|e| NyxError::Upstream(format!("h3 recv_data: {e}")))?
    {
        body_bytes.extend_from_slice(chunk.chunk());
    }

    endpoint.close(0u32.into(), b"done");
    drive.abort();

    let elapsed_ms = start.elapsed().as_millis() as u64;

    use base64::Engine;
    Ok(H3Response {
        status,
        http_version: "HTTP/3".to_string(),
        body_size: body_bytes.len(),
        body_b64: base64::engine::general_purpose::STANDARD.encode(&body_bytes),
        headers: header_pairs,
        elapsed_ms,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_non_https_urls() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let err = rt
            .block_on(async { request("GET", "http://example.com/", &[], b"").await })
            .unwrap_err();
        assert!(matches!(err, NyxError::BadRequest(_)));
    }

    #[test]
    fn rejects_invalid_urls() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let err = rt
            .block_on(async { request("GET", "not a url", &[], b"").await })
            .unwrap_err();
        assert!(matches!(err, NyxError::BadRequest(_)));
    }

    #[test]
    fn builds_rustls_client_config_with_h3_alpn() {
        let cfg = rustls_client_config_for_h3().unwrap();
        assert_eq!(cfg.alpn_protocols, vec![b"h3".to_vec()]);
    }
}
