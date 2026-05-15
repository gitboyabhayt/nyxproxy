# NyxProxy

> **AI-driven, open-source web application security testing suite.** A modern, Rust-powered alternative to Burp Suite — faster to launch, lighter on memory, and shipped with a multi-provider AI gateway out of the box.

[![Desktop CI](https://github.com/gitboyabhayt/nyxproxy/actions/workflows/desktop.yml/badge.svg)](https://github.com/gitboyabhayt/nyxproxy/actions/workflows/desktop.yml)
[![Backend CI](https://github.com/gitboyabhayt/nyxproxy/actions/workflows/backend.yml/badge.svg)](https://github.com/gitboyabhayt/nyxproxy/actions/workflows/backend.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Code of Conduct](https://img.shields.io/badge/contributor%20covenant-2.1-5e60ce.svg)](CODE_OF_CONDUCT.md)
[![Built with Tauri](https://img.shields.io/badge/built%20with-Tauri%202-24c8db.svg)](https://v2.tauri.app/)
[![Built with Rust](https://img.shields.io/badge/built%20with-Rust-orange.svg)](https://www.rust-lang.org/)
[![Backend Python 3.11+](https://img.shields.io/badge/backend-python%203.11%2B-3776ab.svg)](apps/backend/pyproject.toml)
[![PRs welcome](https://img.shields.io/badge/PRs-welcome-success.svg)](CONTRIBUTING.md)

> Drop-in Burp Suite alternative — AI-first, open source, free, native.

## Table of contents

- [Highlights](#highlights)
- [Hosted backend](#hosted-backend)
- [Feature matrix](#feature-matrix)
- [Install](#install)
- [First run](#first-run)
- [Architecture](#architecture)
- [Development](#development)
- [Deploying the backend to Render](#deploying-the-backend-to-render)
- [Configuration reference](#configuration-reference)
- [Roadmap](#roadmap-post-10)
- [Security](#security) · [SECURITY.md](SECURITY.md)
- [Contributing](#contributing) · [CONTRIBUTING.md](CONTRIBUTING.md)
- [Code of Conduct](CODE_OF_CONDUCT.md)
- [Architecture docs](docs/architecture/overview.md)
- [Feature docs](docs/features/README.md)
- [License](#license)

---

## Highlights

- **Intercepting HTTPS proxy** in Rust — on-the-fly CA cert generation, `rustls`-based TLS, HTTP/1.1 + HTTP/2 ready, low memory footprint, no JVM.
- **Burp-style dark UI** in React + TypeScript inside a Tauri 2 shell — native installers for Windows (NSIS / MSI), Linux (deb / AppImage), and a portable dev experience on macOS.
- **AI Assistant** built into every tool — explain requests, suggest fuzz payloads, surface likely vulnerabilities. Backed by a **hosted FastAPI gateway** that fans out to **9 providers** (Groq, OpenRouter, HuggingFace, Cloudflare Workers AI, GitHub Models, NVIDIA NIM, Bytez, Google Gemini, Ollama). Users don't need their own keys to get started — the hosted backend ships with free-tier provider keys.
- **Full Burp-equivalent tooling**: Repeater · Intruder (sniper / battering-ram / pitchfork / cluster-bomb) · Decoder · Comparer · Logger · Sequencer · Spider · Passive + active scanner · Macros · Plugin API · HTTP-only Collaborator · PDF/HTML reporting.
- **Responsive UI** — works on a 4K monitor and a 720-pixel-wide window. Mobile-friendly responsive breakpoints for narrow Tauri windows or browser dev preview.
- **Deterministic browser preview** — `npm run dev` ships a faithful in-memory mock of every Tauri command so the UI renders meaningfully outside the desktop shell.

---

## Hosted backend

A reference backend deployment is available at:

> **https://nyxproxy-backend.onrender.com**

The desktop app talks to this URL by default on a fresh install — no extra configuration required. The User Options page in the app exposes a **Test connection** button that probes `/healthz` and reports latency in real time, so you can verify connectivity at a glance.

You can also point the app at your own self-hosted backend (see [Deploying the backend to Render](#deploying-the-backend-to-render)) or override the default at build time:

```bash
# Bake your own default into a custom desktop binary
NYXPROXY_BACKEND_URL="https://my-backend.example.com" cargo tauri build
```

---

## Screenshots

| Dashboard | Repeater | Intruder | AI Assistant |
| --- | --- | --- | --- |
| `docs/screenshots/dashboard.png` | `docs/screenshots/repeater.png` | `docs/screenshots/intruder.png` | `docs/screenshots/ai.png` |

*(Screenshots will be added once the binary builds are uploaded to the GitHub Releases page.)*

---

## Feature matrix

| Tool | Status | Notes |
| --- | --- | --- |
| **Intercepting proxy** | Phase 1 | HTTP/1.1 + HTTPS via on-the-fly cert generation, history persisted to `~/.nyxproxy/history.jsonl`. |
| **Intercept queue** | Phase 1 | Pause-style modal — edit/forward/drop individual requests. |
| **Repeater** | Phase 1 | Edit any field, resend, diff against the original response. |
| **Decoder** | Phase 2 | URL / base64 / HTML / hex / Unicode / JWT decode + smart auto-detect. |
| **Comparer** | Phase 2 | Word- and line-level diff. |
| **Logger** | Phase 2 | Tail-style stream of every flow, with column filters. |
| **Intruder** | Phase 2 | Sniper / battering-ram / pitchfork / cluster-bomb. Payload templates + grep-match scoring. |
| **Spider** | Phase 3 | Async, respects `robots.txt`, scope-aware, breadth-first BFS with hit log. |
| **Passive scanner** | Phase 3 | Heuristics for missing security headers, verbose errors, mixed content, etc. |
| **Active scanner** | Phase 3 | AI-augmented payload generation feeding into a deterministic attack runner. |
| **Sequencer** | Phase 4 | Token-entropy report against a captured response. |
| **Collaborator** | Phase 4 | HTTP-only OOB callback server (`/collaborator/c/<session>`) with ring-buffered pings. |
| **Plugin API** | Phase 4 | JS plugins via embedded `boa_engine`; sandboxed `on_request`/`on_response` hooks. |
| **Macros** | Phase 5 | Recorded chains of requests for auth + state replay. |
| **Reporting** | Phase 5 | Markdown / HTML / PDF export of selected issues. |
| **AI Assistant** | Phase 1–5 | Single panel; switches between Explain / Vulns / Payloads / Chat modes per tool. |
| **JWT toolkit** | Phase 6 | Decode/encode, HS256 brute-force, `alg=none` generator, RSA↔HMAC confusion detection. See [docs/features/jwt-toolkit.md](docs/features/jwt-toolkit.md). |
| **Workspaces** (.nyxproxy) | Phase 6 | Save and reload an entire session — history + scope + issues + notes — as a zstd-compressed file with the `NYXPRJ` magic header. |
| **OWASP Top 10 mapping** | Phase 6 | Every finding is auto-categorised against the OWASP Top 10 (2021) — both client-side (Rust) and server-side (`/findings/categorize-owasp`). |
| **CVE / CWE mapping** | Phase 6 | Backend route `/findings/map-cve` returns deterministic CVE/CWE associations for a finding description (offline, no API call). |
| **Risk scoring** | Phase 6 | Deterministic 0–100 score combining severity, confidence and OWASP-category bias; aggregate summary per workspace. |
| **Command palette (Ctrl+K)** | Phase 6 | Fuzzy command search over every page and action; recents tracked in `localStorage`. |
| **Recorded Playwright macros** | Batch 4 | Import `npx playwright codegen` `.spec.ts` traces and replay them as authenticated login macros. See [docs/features/recorded-macros.md](docs/features/recorded-macros.md). |
| **Cloud sync (Supabase)** | Batch 4 | Opt-in workspace sync with optimistic-concurrency revisions. Returns `feature_disabled` when not configured. See [docs/features/cloud-sync.md](docs/features/cloud-sync.md). |
| **Live multi-user collaboration** | Batch 4 | WebSocket signalling room with presence, chat, and live cursor events. No persistence — pure real-time. See [docs/features/collaboration.md](docs/features/collaboration.md). |
| **Distributed scan fleet** | Batch 4 | Horizontally scale the passive scanner across `nyxproxy-worker` processes via a SQLite-backed job queue. See [docs/features/distributed-scan.md](docs/features/distributed-scan.md). |

> All Phase 1-6 and Batch 4 features are landed on the `main` branch — this README's *Roadmap* below tracks what's queued next.

---

## Install

### Linux (Debian / Ubuntu)

```bash
# Once the GitHub release is published:
curl -LO https://github.com/gitboyabhayt/nyxproxy/releases/latest/download/nyxproxy_0.1.0_amd64.deb
sudo apt install ./nyxproxy_0.1.0_amd64.deb
```

The deb declares `libwebkit2gtk-4.1-0` and `libgtk-3-0` as dependencies, so `apt` will pull them in automatically.

### Linux (AppImage)

```bash
curl -LO https://github.com/gitboyabhayt/nyxproxy/releases/latest/download/nyxproxy_0.1.0_amd64.AppImage
chmod +x nyxproxy_0.1.0_amd64.AppImage
./nyxproxy_0.1.0_amd64.AppImage
```

### Windows

Two installer flavours are produced by CI:

- **`NyxProxy-0.1.0-setup.exe`** — NSIS installer, per-user install (no admin prompt), bundled WebView2 bootstrapper, customisable install location.
- **`NyxProxy_0.1.0_x64_en-US.msi`** — MSI for enterprise deployment via Group Policy / Intune.

Both register Start Menu and Desktop shortcuts and add an entry to Add/Remove Programs.

### From source (any platform)

See [Development](#development).

---

## First run

1. **Launch NyxProxy.** A workspace directory is created at `~/.nyxproxy/` with `ca/`, `history.jsonl`, and `settings.json`.
2. **Install the CA.** Go to **User Options → Certificates → Download CA (PEM)** and trust the file in your browser (or OS trust store) so HTTPS interception is transparent.
3. **Configure your browser proxy** to `127.0.0.1:8080` (the default — change in **Proxy → Options**).
4. **Verify the backend.** On **User Options**, the *Backend URL* defaults to `https://nyxproxy-backend.onrender.com`. Hit **Test connection** — you should see `status=ok · <latency> ms`.
5. **Start the proxy.** Title bar → **Start proxy**. Traffic now flows through NyxProxy. Open the Proxy tab to watch flows arrive in real time.

---

## Architecture

```
┌────────────────────────────────────────────────────────────────────────┐
│  Tauri desktop app                                                     │
│                                                                        │
│   ┌────────────────────────┐   IPC   ┌─────────────────────────────┐  │
│   │  React + TypeScript    │ ───────▶│  Rust (tauri::command)      │  │
│   │  (Vite + Zustand)      │ ◀────── │  - settings store           │  │
│   │  - 12-tool dark UI     │  events │  - history (JSONL)          │  │
│   └────────────────────────┘         │  - intruder / spider /      │  │
│              │                       │    scanner / sequencer      │  │
│              │ fetch                 │  - plugins (boa_engine)     │  │
│              ▼                       └──────────────┬──────────────┘  │
│   ┌────────────────────────┐                        │                 │
│   │  nyxproxy-core         │  hyper + rustls        │                 │
│   │  (intercepting proxy)  │ ──────────────────────▶│ upstream HTTPS  │
│   └────────────────────────┘                        │                 │
└─────────────┬────────────────────────────────────────────────────────┘
              │ AI requests
              ▼
┌────────────────────────────────────────────────────────────────────────┐
│  FastAPI gateway  (apps/backend, hosted on Render)                     │
│  /v1/chat/completions      OpenAI-compatible                           │
│  /v1/analyze/request       Burp-style "explain this request"           │
│  /v1/analyze/vulns         Surface likely vulnerabilities              │
│  /v1/analyze/payloads      Generate fuzzing payloads for Intruder      │
│  /collaborator/*           HTTP-only OOB callback server               │
└────────────────────────────────────────────────────────────────────────┘
                │
                ├─▶ Groq · OpenRouter · HuggingFace · Cloudflare · GitHub Models
                ├─▶ NVIDIA NIM · Bytez · Google Gemini
                └─▶ Ollama (local fallback)
```

---

## Development

### Prerequisites

- **Rust** stable (`rustup default stable`)
- **Node.js** 20+ and **npm** 10+
- **Python** 3.11+ (for the backend)
- Linux: `libwebkit2gtk-4.1-dev`, `libgtk-3-dev`, `libssl-dev`, `librsvg2-dev`, `libayatana-appindicator3-dev`, `pkg-config`

### Backend

```bash
cd apps/backend
python -m venv .venv && source .venv/bin/activate
pip install -e ".[dev]"
cp .env.example .env  # add any provider keys you have
uvicorn nyxproxy_backend.main:app --reload --port 8000
```

Tests: `pytest -q`. Lint/format: `ruff check . && ruff format --check .`.

### Desktop app

```bash
cd apps/desktop
npm install
npm run tauri dev          # launches the native shell + Vite dev server
# or, browser-only preview of the UI with mock IPC:
npm run dev
```

Quality gates the CI runs:

```bash
# Frontend
npx tsc --noEmit
npx vite build

# Rust
cargo test -p nyxproxy-core --release
cargo check -p nyxproxy-app --release
```

### Packaging

```bash
cd apps/desktop
npm run tauri build       # outputs deb + AppImage on Linux, MSI + NSIS on Windows
```

Artefacts land in `apps/desktop/src-tauri/target/release/bundle/`.

---

## Deploying the backend to Render

1. Fork this repo.
2. In Render, click **New → Blueprint** and point at the fork — Render reads [`render.yaml`](render.yaml) and provisions the service.
3. In the Render dashboard for the service, open **Environment** and paste whichever provider keys you have:
   `GROQ_API_KEY`, `OPENROUTER_API_KEY`, `HF_TOKEN`, `CLOUDFLARE_API_TOKEN`, `CLOUDFLARE_ACCOUNT_ID`, `GITHUB_MODELS_TOKEN`, `NVIDIA_API_KEY`, `BYTEZ_API_KEY`, `GEMINI_API_KEY`.
   *Any single key is enough to get the AI features working — the gateway routes only to providers that have credentials configured.*
4. (Optional) Lock the gateway down by setting `BACKEND_API_TOKEN` and entering the same value under **User Options → Backend bearer token** in the desktop app.
5. In the desktop app, open **User Options**, paste your Render URL into **Backend URL**, click **Test connection**, then **Save settings**.

---

## Configuration reference

| Setting | Stored as | Notes |
| --- | --- | --- |
| Backend URL | `settings.json` → `backend_url` | Default: `https://nyxproxy-backend.onrender.com`. Override at build time with `NYXPROXY_BACKEND_URL`. Legacy `http://127.0.0.1:8765` from older builds is auto-migrated on launch. |
| Backend bearer token | `settings.json` → `backend_token` | Sent as `Authorization: Bearer <token>` when present. |
| Default AI provider | `settings.json` → `default_ai_provider` | Falls back to `groq` if not set or if the chosen provider has no key configured server-side. |
| Proxy listen address | `settings.json` → `proxy.listen_addr` | Default: `127.0.0.1:8080`. |
| CA bundle | `~/.nyxproxy/ca/cert.pem` + `key.pem` | Auto-generated on first launch. The private key is `0600` on Linux. Never share these files. |
| History | `~/.nyxproxy/history.jsonl` | One JSON-encoded flow per line. Safe to delete to start fresh. |

---

## Roadmap (post-1.0)

- macOS DMG bundling (currently Windows + Linux only).
- HTTP/2 upgrade negotiation in the proxy core.
- ~~Recorded-login replay via Playwright-style session capture.~~ — landed in batch 4 (`docs/features/recorded-macros.md`).
- ~~Team-mode shared state over a Realtime channel.~~ — landed in batch 4 (`docs/features/collaboration.md` + opt-in cloud sync at `docs/features/cloud-sync.md`).
- ~~Distributed / horizontal scanner fleet.~~ — landed in batch 4 (`docs/features/distributed-scan.md`).
- Native plugin SDK for Rust + WASM in addition to the current JS plugin runtime.

---

## Security

NyxProxy is an offensive security tool — **only intercept traffic you are authorised to test.** The CA private key never leaves the local machine and is stored with restrictive permissions (`0600` on Linux, ACL'd on Windows). All AI calls go through the backend you configure; the desktop app never embeds provider API keys.

If you discover a security issue in NyxProxy itself, please open a private security advisory rather than a public issue.

---

## Contributing

PRs welcome! Please:

1. Open an issue first for non-trivial changes.
2. Run the local quality gates (`cargo test`, `npx tsc --noEmit`, `npx vite build`, `ruff check .`, `pytest`) before pushing.
3. Add tests for new Rust behaviour where practical.
4. Avoid touching `apps/backend/.env` or committing real API keys.

---

## License

MIT — see [LICENSE](LICENSE). Built by NyxProxy Contributors.
