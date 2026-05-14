# HTTP/2 + HTTP/3 support (Feature G)

NyxProxy's MITM tunnel can now negotiate **HTTP/2** over its TLS leaf
certificates, and the Repeater can issue **HTTP/3 (QUIC)** requests directly
to upstream servers.

## What changed

### HTTP/2 in the MITM (`apps/desktop/crates/nyxproxy-core/src/proxy.rs`)

- The MITM TLS `ServerConfig` now advertises `h2, http/1.1` via ALPN by
  default. Clients (browsers, mobile apps) will pick whichever protocol they
  prefer; modern Chromium-based browsers will pick `h2`.
- After the TLS handshake, NyxProxy inspects the negotiated ALPN protocol:
  - `h2` → serves the inner connection using
    [`hyper_util::server::conn::auto::Builder::http2`].
  - anything else → falls back to the existing HTTP/1.1 path
    (`hyper::server::conn::http1`).
- The whole behaviour is gated behind two new fields on `ProxyConfig`:

```rust
pub struct ProxyConfig {
    pub listen_addr: String,
    pub intercept_enabled: bool,
    pub scope_include: Vec<String>,
    pub scope_exclude: Vec<String>,
    pub enable_http2: bool, // default true
    pub enable_http3: bool, // default false
}
```

Disabling `enable_http2` flips ALPN back to `http/1.1` only — useful when
debugging legacy stacks that don't speak h2.

### HTTP/3 upstream client (`apps/desktop/crates/nyxproxy-core/src/http3.rs`)

A new `http3` module exposes a single async function:

```rust
pub async fn request(
    method: &str,
    url: &str,
    headers: &[(String, String)],
    body: &[u8],
) -> NyxResult<H3Response>;
```

Internally it spins up a `quinn::Endpoint`, performs the QUIC handshake using
a `rustls::ClientConfig` with `h3` ALPN, then drives an `h3::client` connection
to a completion. The response status, headers, body (base64-encoded), and
elapsed latency are returned to the caller.

Notes:

- Cert verification uses `webpki-roots` so it doesn't depend on the OS trust
  store.
- Only `https://` URLs are accepted — HTTP/3 has no plaintext mode.
- The endpoint is closed gracefully after the response is consumed.

### Tauri command + frontend wiring

- New Tauri command `http3_send` in
  `apps/desktop/src-tauri/src/commands.rs`.
- New TypeScript wrapper `Http3Api.send(args)` in
  `apps/desktop/src/tauri/api.ts`.
- The Repeater page now has a **Send /h3** button next to **Send** — clicking
  it dispatches the current draft over HTTP/3 and surfaces the response
  (status, headers, body) in the existing response panel.
- The Proxy page now shows **HTTP/2** and **HTTP/3** checkboxes in the
  intercept toolbar so users can toggle the MITM behaviour without editing
  config files.

## Why this matters

- **Burp parity**: Burp Suite has supported h2 in its MITM for years. Without
  h2 ALPN, modern browsers either downgrade or refuse the connection — making
  the proxy effectively unusable for any modern API.
- **Modern API testing**: gRPC-over-HTTP/2, Server-Sent-Events over h2, and
  HTTP/3-only CDNs all become testable.
- **Foundation for h2-aware fuzzing**: streams are first-class in h2, so a
  future PR can add per-stream fuzzing, priority abuse tests, and HPACK
  manipulation — none of which are reachable behind a strict h1 proxy.

## Test coverage

The `nyxproxy-core` crate now has **97 unit tests** including:

- `proxy::tests::server_config_advertises_h2_and_h1_when_http2_enabled`
- `proxy::tests::server_config_advertises_only_h1_when_http2_disabled`
- `proxy::tests::default_config_enables_http2_only`
- `http3::tests::rejects_non_https_urls`
- `http3::tests::rejects_invalid_urls`
- `http3::tests::builds_rustls_client_config_with_h3_alpn`

## Roadmap (not yet shipped)

- HTTP/3 in the **inbound** path (the proxy listening on UDP/443 and
  re-issuing requests upstream). Today only the upstream side speaks h3.
- HPACK / QPACK aware viewers in the request inspector.
- Per-stream timeline view for h2 multiplexed requests.
- Connection pooling for h3 (currently each `Http3Api.send` opens a fresh
  QUIC connection).
