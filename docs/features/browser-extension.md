# Browser extension (Feature EE)

Status: **Shipped** — `extensions/browser/`

## What it does

A Manifest V3 browser extension (Chrome / Edge / Brave / Chromium) that
adds a "Send to NyxProxy" right-click menu item and a toolbar button.
Clicking either sends the relevant URL to NyxProxy via the local HTTP
bridge — no manual proxy config or browser CA install required for this
specific flow.

## Architecture

```
┌──────────────────┐    fetch(127.0.0.1:8090)    ┌──────────────────────┐
│ Chromium tab     │ ────────────────────────────▶│  bridge.rs (Tauri)   │
│  background.js   │                              │  POST /api/v1/...    │
└──────────────────┘                              │   import-url         │
                                                  │   import-flow        │
                                                  │   ping               │
                                                  └──────────┬───────────┘
                                                             │
                                                             ▼
                                                      HistoryStore
                                                      (live)
```

The bridge:

* Binds to `127.0.0.1:8090` by default (config in
  [`bridge.rs`](../../apps/desktop/crates/nyxproxy-core/src/bridge.rs)).
* Refuses any request larger than 4 MiB.
* Sets permissive CORS headers because Manifest V3 service workers send
  the JSON body with `content-type: application/json` which triggers a
  preflight.
* Inserts the resulting flow into the live `HistoryStore`, so it shows up
  in Logger / History immediately, tagged `source:browser-ext`.

## Endpoints

| Method | Path                  | Behaviour                                   |
|--------|-----------------------|---------------------------------------------|
| GET    | `/api/v1/ping`        | Returns `{ ok: true, data: { version } }`. |
| POST   | `/api/v1/import-url`  | Bridge fetches the URL, captures response. |
| POST   | `/api/v1/import-flow` | Inserts a serialised `HttpFlow` verbatim. |

## Install (Chrome / Edge / Brave)

1. Open `chrome://extensions`.
2. Toggle **Developer mode**.
3. Click **Load unpacked**.
4. Pick the `extensions/browser` folder.
5. Right-click any page → **Send page to NyxProxy**.

NyxProxy must be running. The extension will show a toast when sending
succeeds or fails.

## Tests

`apps/desktop/crates/nyxproxy-core/src/bridge.rs` ships with three
hyper-driven integration tests — all passing:

* `ping_returns_version` — boots the bridge, calls `/api/v1/ping`,
  asserts `ok=true` and a `version` field is present.
* `import_flow_inserts_into_history` — POSTs an `HttpFlow`, asserts it
  appears in the `HistoryStore` with the `source:browser-ext` tag.
* `invalid_route_returns_404_json` — asserts unknown routes return a
  JSON 404 (not an HTML default), preserving the API contract for the
  extension.

```
$ cargo test -p nyxproxy-core --lib bridge
test result: ok. 3 passed; 0 failed
```

## Future work

* Firefox / Safari builds (`web-ext` package).
* "Send selected text as raw HTTP" context menu (paste-as-request).
* OAuth handshake replay (record the OAuth dance once, replay it on
  demand from NyxProxy).
