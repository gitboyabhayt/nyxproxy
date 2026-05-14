# HTTP/2 in the MITM hop

Status: **Shipped** in `apps/desktop/crates/nyxproxy-core/src/proxy.rs`
(behind the standard `CONNECT` tunnel — no feature flag needed).

## What it does

NyxProxy's intercepting proxy now serves the inner (decrypted) connection
using **HTTP/2** when the client offers it via ALPN, and falls back to
**HTTP/1.1** otherwise. WebSocket upgrades stay on HTTP/1.1 (because
RFC 6455 is HTTP/1.1 only — RFC 8441's `:protocol` extension for
WebSocket over HTTP/2 is not yet broadly used).

## Architecture

1. The client (typically a browser) opens a `CONNECT host:port HTTP/1.1`
   tunnel to NyxProxy's outer HTTP/1.1 listener.
2. NyxProxy replies `200 OK` and the outer connection is upgraded to a
   raw TCP relay.
3. NyxProxy performs a **server-side TLS handshake** on the relay using
   a leaf certificate minted by `CertAuthority::leaf_for(host)`.
4. The `ServerConfig` advertises ALPN protocols `["h2", "http/1.1"]`,
   in priority order (`h2` first).
5. After the handshake completes, NyxProxy inspects the negotiated ALPN
   protocol via `tls_stream.get_ref().1.alpn_protocol()`:
   * `b"h2"` → serve via `hyper::server::conn::http2::Builder` (with
     `TokioExecutor`).
   * Anything else (including `None`, `b"http/1.1"`) → serve via
     `hyper::server::conn::http1::Builder` (with `.with_upgrades()` so
     WebSocket still works).

The same `service_fn` (which dispatches to `serve_intercepted`) is used
on both code paths, so capture, history, intercept-queue, scope filters
and AI features all work uniformly for HTTP/1.1 and HTTP/2 flows.

## HTTP/3 (QUIC)

HTTP/3 **cannot** be intercepted via a standard `CONNECT` tunnel
because `CONNECT` tunnels TCP, while QUIC runs over UDP. Browsers
discover HTTP/3 via the `Alt-Svc` (and legacy `Alternate-Service`)
response headers. If those headers leak through the proxy, the browser
opens a direct QUIC connection to the upstream and bypasses NyxProxy
entirely.

To prevent that bypass, NyxProxy **strips `Alt-Svc` and
`Alternate-Service` headers from every response** in
`forward_capture`. Browsers fall back to HTTP/2 (now intercepted) or
HTTP/1.1 (already intercepted). This matches Burp Suite Pro's behaviour.

For upstream HTTP/3 (proxy → server hop), `reqwest` supports HTTP/3 via
its experimental `http3` feature. It is not enabled by default because
QUIC platform support varies and rustls + quinn add ~3 MB of binary
size; we may opt it in behind a runtime config flag in a later release.

## Tests

End-to-end tests in
`apps/desktop/crates/nyxproxy-core/tests/proxy_integration.rs`:

* `mitm_negotiates_http2_via_alpn` — opens CONNECT, performs TLS
  handshake with `h2,http/1.1` ALPN, asserts negotiated protocol is
  `h2`.
* `mitm_falls_back_to_http1_when_client_offers_h1_only` — opens
  CONNECT, performs TLS handshake with `http/1.1` ALPN only, asserts
  negotiated protocol is `http/1.1`.

Plus a unit test in `proxy::tests`:

* `server_config_advertises_h2_and_h1_alpn` — verifies the
  `ServerConfig` built per-host always advertises ALPN
  `["h2", "http/1.1"]`.

## Why this matters

Burp Suite Pro speaks HTTP/2 in its MITM hop since 2020. Browsers
strongly prefer HTTP/2 for any modern TLS connection, and a proxy that
forces them back to HTTP/1.1 will either:
* introduce subtle protocol-fidelity bugs (e.g. requests that work
  natively but break through the proxy), or
* miss multiplexed streams entirely.

With this change NyxProxy's traffic interception is **protocol-faithful
for the >99% of HTTPS traffic that uses HTTP/2 today**.

## Limitations

* WebSocket upgrades still require HTTP/1.1 (intentional — RFC 8441 is
  not widely deployed by browsers).
* HTTP/3 is not intercepted; it is blocked by header stripping.
* HTTP/2 server push (`PUSH_PROMISE`) is not specifically modelled in
  history view; pushed responses are recorded as normal flows but the
  causal "pushed by request X" link is not yet rendered in the UI.
