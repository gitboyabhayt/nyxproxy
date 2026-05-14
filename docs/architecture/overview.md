# NyxProxy — Architecture Overview

NyxProxy is split into three independently-deployable layers. Each layer has a clear contract with the other two, so they can be developed, tested and scaled in isolation.

```
                           ┌─────────────────────────────────────────────┐
                           │             FastAPI AI gateway              │
                           │   apps/backend/nyxproxy_backend             │
                           │   Hosted at nyxproxy-backend.onrender.com   │
                           │                                             │
                           │   /healthz       /providers   /analyze      │
                           │   /chat          /collaborator              │
                           └──────────────┬──────────────────────────────┘
                                          │ HTTPS  (Bearer optional)
                                          │
                ┌─────────────────────────┴──────────────────────────────┐
                │                Tauri desktop app                       │
                │              apps/desktop                              │
                │                                                        │
                │   ┌──────────────────────────────────────┐             │
                │   │       React + TypeScript UI          │             │
                │   │       src/  (Zustand store)          │             │
                │   └────────────┬─────────────────────────┘             │
                │                │  Tauri IPC                            │
                │   ┌────────────┴─────────────────────────┐             │
                │   │   Rust integration (src-tauri/)      │             │
                │   │   commands.rs · state.rs · settings  │             │
                │   └────────────┬─────────────────────────┘             │
                │                │  in-process                           │
                │   ┌────────────┴─────────────────────────┐             │
                │   │   nyxproxy-core  (crates/...)        │             │
                │   │   proxy · ca · history · repeater    │             │
                │   │   intruder · scanner · spider        │             │
                │   │   sequencer · decoder · macros       │             │
                │   │   plugins · report                   │             │
                │   └──────────────────────────────────────┘             │
                └────────────────────────────────────────────────────────┘
                                          │
                                          │ HTTP(S) + WS + HTTP/2 + HTTP/3
                                          ▼
                            ┌─────────────────────────────┐
                            │        Target webapps       │
                            └─────────────────────────────┘
```

## 1. Backend (`apps/backend/`)

A small, stateless FastAPI service that acts as an **AI provider gateway**. It accepts requests from the desktop app and forwards them to whichever LLM provider is configured (Groq, OpenRouter, Gemini, HuggingFace, Cloudflare, GitHub Models, NVIDIA, Bytez, Ollama).

Key routes:

| Route | Purpose |
|---|---|
| `GET /healthz` | Liveness probe used by the desktop **Test connection** button |
| `GET /providers` | Lists available providers + models the backend has API keys for |
| `POST /chat` | One-shot completion (used by the AI Assistant page) |
| `POST /analyze` | Structured analysis of a request/response (used by the AI panel in Repeater / Logger) |
| `*  /collaborator/*` | Out-of-band interaction (DNS pingback) for findings like SSRF |

The backend never stores user data. It is safe to host shared on Render (default) or self-host on any environment that runs Python 3.11+.

## 2. Desktop integration (`apps/desktop/src-tauri/`)

The Rust side of Tauri. Owns:

* Window management, system tray, auto-updates
* The Tauri command registry — every IPC call from the frontend lands here
* The Zustand-mirrored settings file at `~/.nyxproxy/settings.json`
* CA management (generate, export, install hint)
* Wire-up to `nyxproxy-core`

## 3. nyxproxy-core (`apps/desktop/crates/nyxproxy-core/`)

A `no_std`-free, fully-async Rust crate that contains every security feature. It can be embedded in the desktop app or, in the future, run headless on a server for distributed scans (Feature K).

Modules:

| Module | Responsibility |
|---|---|
| `proxy.rs` | HTTP(S) MITM listener — based on `hyper` + `tokio-rustls` |
| `ca.rs` | Root CA generation, signing per-host leaves on the fly |
| `intercept.rs` | Per-request hold / forward / drop state machine |
| `history.rs` | JSONL persistence + tail-load on startup |
| `repeater.rs` | Replay & edit captured requests |
| `intruder.rs` | Sniper / battering-ram / pitchfork / cluster-bomb attack engines |
| `scanner.rs` | Passive + active vulnerability checks |
| `spider.rs` | Crawler with scope filter |
| `sequencer.rs` | Token entropy analysis |
| `decoder.rs` | URL / base64 / hex / HTML / JWT / JSON encoding round-trip |
| `macros.rs` | Chained-request playback with variable extraction |
| `plugins.rs` | Out-of-process WASM / native plugin host |
| `report.rs` | HTML & Markdown report generator |
| `model.rs` | Shared types (Flow, Issue, Severity, etc.) |

## 4. Frontend (`apps/desktop/src/`)

React 18 + TypeScript + Zustand. Pages map 1:1 to nyxproxy-core tools. Styling is hand-written CSS (no Tailwind) for tight control over the Burp-inspired dark theme.

Cross-cutting concerns:

* **State:** `state/store.ts` is a single Zustand store mirroring all Rust state
* **IPC:** `tauri/api.ts` wraps every Tauri command in a typed function
* **Toasts:** `components/Toasts.tsx` consumes a queue from the store
* **Responsive:** 5 media-query breakpoints at 1200 / 960 / 720 px, plus `pointer:coarse` and `prefers-reduced-motion`

## Data flow example — a single intercepted request

```
1. Browser sends GET https://example.com
2. proxy.rs accepts the connection, ca.rs signs an example.com leaf cert
3. proxy.rs constructs a Flow{request, ...} and emits it on the Tokio channel
4. src-tauri commands.rs forwards it to the frontend via Tauri event
5. state/store.ts appends to `history[]`, the Logger page re-renders
6. If user clicks "Send to Repeater" → frontend dispatches a Tauri command
7. repeater.rs replays the request, response goes back via the same channel
8. If user clicks "AI explain" → frontend POSTs to /analyze on the backend
9. Backend forwards to the configured LLM, streams back the explanation
```

## Storage layout (`~/.nyxproxy/`)

```
~/.nyxproxy/
├── settings.json          # User preferences + backend URL + provider keys (encrypted at rest)
├── version.txt            # Last-run version (for migrations)
├── ca/
│   ├── nyxproxy-ca.crt    # PEM-encoded root certificate (export to browser)
│   └── nyxproxy-ca.key    # Private key (chmod 600)
├── history.jsonl          # Append-only log of every captured flow
├── workspaces/            # Saved .nyxproxy project files
├── macros/                # Recorded macros (incl. Playwright JSON)
└── logs/                  # Diagnostic logs (rolling, 7-day retention)
```

## CI / Release

`.github/workflows/desktop.yml` builds the Tauri app and produces installers for:

* Windows: NSIS (`.exe`) + MSI (`.msi`)
* Linux: `.deb` + AppImage
* (macOS planned)

`.github/workflows/backend.yml` runs ruff + pytest. The hosted backend on Render redeploys automatically on push to `main`.
