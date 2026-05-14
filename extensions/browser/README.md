# Send to NyxProxy — browser extension

Manifest V3 extension for Chrome / Edge / Brave / Chromium that adds right-click
"Send to NyxProxy" context menu items, plus a toolbar button that sends the
current tab.

## How it works

The extension talks to a tiny local HTTP bridge exposed by the NyxProxy
desktop app at `http://127.0.0.1:8090`. Endpoint:

```
POST /api/v1/import-url
{
  "url": "https://example.com",
  "method": "GET",
  "tags": ["source:browser-ext"]
}
```

The bridge fetches the URL server-side and appends a request/response pair
to the NyxProxy history store. The CORS preflight is handled, so a regular
`fetch()` from the extension service worker just works.

Source of the bridge: `apps/desktop/crates/nyxproxy-core/src/bridge.rs`.

## Install (unpacked)

1. Open `chrome://extensions` (or `edge://extensions`, `brave://extensions`).
2. Toggle on **Developer mode**.
3. Click **Load unpacked**.
4. Select the `extensions/browser/` directory.
5. Open NyxProxy. The toolbar icon should turn into a usable button.
6. Right-click any page → **Send page to NyxProxy**, or right-click any link
   → **Send link to NyxProxy**.

If NyxProxy is not running on the default port, open the extension's
options page (right-click the toolbar icon → Options) and set the bridge
base URL to whatever you bound the bridge to.

## Firefox

The same manifest works on Firefox 109+ with the `browser_specific_settings`
extension; we'll ship a Firefox build (`web-ext` packaged) in a follow-up
release. The core background script and options page are framework-free
and run unchanged.
