# Embedded browser (Feature DD)

NyxProxy can open a separate Tauri webview window pre-configured to
route every request through the running NyxProxy listener — the same
killer convenience Burp Pro's "Open Burp's Browser" gives you, with
zero manual proxy setup.

## How it works

Tauri 2 exposes a per-webview proxy URL. We create a
`WebviewWindowBuilder` with:

```rust
WebviewWindowBuilder::new(&app, label, WebviewUrl::External(target))
    .title("NyxProxy Browser — proxy http://127.0.0.1:8080")
    .inner_size(1280.0, 800.0)
    .proxy_url(proxy_parsed)
    .build()?;
```

The native webview (WebKitGTK on Linux, WebView2 on Windows, WKWebView
on macOS) routes the entire navigation through our proxy. Cookies,
storage, and DevTools are all scoped to that window — no contamination
of the main app's webview.

## CA trust

The embedded webview uses the **OS** trust store. NyxProxy's CA
certificate must be added there so HTTPS interception works without
warnings:

- **Linux**: Copy `~/.nyxproxy/ca/nyxproxy-ca.pem` to
  `/usr/local/share/ca-certificates/` and run `sudo update-ca-certificates`.
- **macOS**: Open Keychain Access → System → drag the PEM in → set
  trust to *Always Trust*.
- **Windows**: Double-click the `.pem`, install into *Trusted Root
  Certification Authorities*.

For one-off testing without OS-level trust, click the certificate
warning's *Proceed* button — same as Burp's behaviour.

## UI

**Project options → Embedded browser**:

1. Enter the target URL.
2. Press *Open browser*.

The window appears, routed through the proxy at the address shown in
the panel header.

## Programmatic access

```ts
import { EmbeddedBrowserApi } from "@/tauri/api";

await EmbeddedBrowserApi.open("https://example.com");
// or override the proxy explicitly:
await EmbeddedBrowserApi.open("https://example.com", "http://127.0.0.1:9000");
```

## Limitations

- DevTools availability depends on the platform webview (WebView2
  enables DevTools by default; WKWebView requires the `developer-extras`
  preference).
- Some sites detect WebView and serve a degraded experience. If you
  need a stealthier client, point an external browser at the proxy
  manually.
