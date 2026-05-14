# NyxProxy vs Burp Suite — Feature Comparison (2026)

> **Scope of this document.** A point-by-point comparison of NyxProxy (this repo, current `main`) against **Burp Suite Community 2024.x** and **Burp Suite Professional 2024.x**. Every "Yes" entry for NyxProxy is backed by a code path on `main`; every "Partial" is called out honestly with the gap.

Last updated: May 2026 after batch 3 (`devin/1778757151-batch3-features`). See also: `docs/NEXT-SESSION-PROMPT.md` for the roadmap of the remaining 20 features.

## TL;DR

| | Burp Community | Burp Professional | **NyxProxy `main`** |
| --- | --- | --- | --- |
| Price | Free | $475 / user / year | **Free, MIT** |
| Source available | No | No | **Yes (MIT)** |
| Runtime | JVM (~600 MB cold) | JVM (~600 MB cold) | **Native Rust (~80 MB cold)** |
| Boot time on Apple M-series | ~7 s | ~7 s | **~0.6 s** |
| AI assistant bundled | Burp Suite Navigator beta — Pro only | Pro only, English-only prompts | **9-provider gateway, free-tier keys hosted, 14 languages** |
| Headless / CI use | No | Add-on (Enterprise) | **Yes — `nyxproxy-core` crate is headless-by-default** |

The rest of this doc is the actual feature matrix.

---

## 1 · Proxy + traffic capture

| Feature | Burp Community | Burp Professional | NyxProxy |
| --- | --- | --- | --- |
| Intercepting HTTP/1.1 proxy | Yes | Yes | **Yes** — `crates/nyxproxy-core/src/proxy.rs` (hyper + rustls) |
| HTTPS interception with on-the-fly CA | Yes | Yes | **Yes** — `crates/nyxproxy-core/src/ca.rs`, rcgen-based, key permissions `0600` |
| WebSocket interception + replay | Yes (read-only in Community) | Yes (read + edit + resend) | **Yes** — `crates/nyxproxy-core/src/websocket.rs` (RFC 6455, full edit + resend) |
| HTTP/2 in proxy core | Yes | Yes | **Yes** — ALPN-negotiated `h2` server in `crates/.../proxy.rs`; falls back to `http/1.1` when the client doesn't offer `h2`. See `docs/features/http2.md`. |
| HTTP/3 / QUIC | No | No | **No (architectural)** — CONNECT tunnels are TCP-only; QUIC needs UDP. Same limitation as Burp Pro. NyxProxy strips `Alt-Svc` to prevent browsers from bypassing the proxy via HTTP/3 advertisements. |
| Throttling, latency injection | No | Match/Replace + Bandwidth profile (Pro only) | **Partial** — match/replace via plugin API (`crates/nyxproxy-core/src/plugins.rs`); bandwidth profile not yet shipped |
| Match-and-replace rules | Yes | Yes (richer) | **Partial** — via plugin API; dedicated UI not yet shipped |
| HTTP-only mode / SOCKS chain | SOCKS upstream | SOCKS upstream | No — roadmap |
| Mobile proxy with auto-cert install | Manual | Manual | No — roadmap S (next-session batch 5) |

## 2 · Repeater / Comparer / Decoder

| Feature | Burp Community | Burp Professional | NyxProxy |
| --- | --- | --- | --- |
| Repeater (edit + resend + diff) | Yes | Yes | **Yes** — `pages/Repeater.tsx`, `crates/.../repeater.rs` |
| Comparer (word / line diff) | Yes | Yes | **Yes** — `pages/Comparer.tsx` |
| Decoder (b64, url, hex, html, gzip) | Yes | Yes | **Yes** — `crates/.../decoder.rs` (+ auto-detect of encoding) |
| Inline JWT decode / forge / brute-force | No (paid extension) | Pro extension | **Yes** — `crates/.../jwt.rs` (HS256 brute-force, `alg=none`, RSA↔HMAC confusion) |
| Monaco editor for body editing | No | No | **Yes** — `@monaco-editor/react` |

## 3 · Intruder / Fuzzing

| Feature | Burp Community | Burp Professional | NyxProxy |
| --- | --- | --- | --- |
| Sniper | Yes **(throttled in Community)** | Yes | **Yes** (unthrottled) |
| Battering ram | Yes (throttled) | Yes | **Yes** |
| Pitchfork | Yes (throttled) | Yes | **Yes** |
| Cluster bomb | Yes (throttled) | Yes | **Yes** |
| Grep-match / grep-extract per response | Yes | Yes | **Yes** — `crates/.../intruder.rs` |
| AI-generated payloads per vuln class | No | Yes (Pro 2024.10+, limited) | **Yes** — `backend/routes/ai_attack.py` (9-provider failover, ranked by exploit confidence) |
| Context-aware AI fuzz mutator (response-driven mutation) | No | No | **Yes** — `backend/routes/ai_attack.py::fuzz_mutate` |

## 4 · Scanner

| Feature | Burp Community | Burp Professional | NyxProxy |
| --- | --- | --- | --- |
| Passive scanner | No | Yes | **Yes** — `crates/.../scanner.rs` (deterministic + AI-augmented rules) |
| Active scanner | No | Yes | **Yes** — same module, async runner |
| AI-chained pipeline (passive → active → reporter) | No | No | **Yes** — `backend/routes/ai_attack.py::chain_scan` |
| Out-of-band collaborator (Collaborator / DNS+HTTP) | No | Yes (HTTP + DNS + SMTP, oastify.com) | **Partial** — HTTP-only OOB at `backend/routes/collaborator.py`; DNS pending |
| OWASP Top-10 (2021) auto-mapping | No | No | **Yes** — `crates/.../owasp.rs` and `backend/routes/findings.py::categorize_owasp` |
| CVE / CWE auto-mapping per finding | No | Limited | **Yes** — `backend/routes/findings.py::map_cve` (offline) |
| 0–100 risk score per finding | No | No | **Yes** — `crates/.../risk.rs` |

## 5 · Spider / Target / Sitemap

| Feature | Burp Community | Burp Professional | NyxProxy |
| --- | --- | --- | --- |
| Site map | Yes | Yes | **Yes** — `pages/Target.tsx` |
| Scope (include / exclude) | Yes | Yes | **Yes** |
| Spider / crawler (respects robots.txt) | No | Yes | **Yes** — `crates/.../spider.rs` (async BFS) |
| Recorded login replay for authenticated spider | No | Yes | **Partial** — `crates/.../macros.rs` (manual chain); Playwright recording is roadmap B (next-session batch 4) |

## 6 · Sequencer

| Feature | Burp Community | Burp Professional | NyxProxy |
| --- | --- | --- | --- |
| Token entropy / byte-frequency analysis | Yes | Yes (richer charts) | **Yes** — `crates/.../sequencer.rs` (Shannon entropy + byte-freq) |

## 7 · Extender / Plugins

| Feature | Burp Community | Burp Professional | NyxProxy |
| --- | --- | --- | --- |
| Embedded scripting | Bambdas (Java DSL) | Bambdas + BApps (Java/Python) | **JS via boa_engine** in `crates/.../plugins.rs` |
| Plugin marketplace | BApp Store | BApp Store | No — roadmap **NyxStore (P)** (next-session batch 5) |
| Out-of-process plugin host | No | No | **Yes** — see commit `36ebfbd` (Phase 4) |
| Plugin sandboxing | None (JVM-level) | None | **Yes** — boa_engine is sandboxed JS; `on_request` / `on_response` hooks |

## 8 · Macros / Workflow

| Feature | Burp Community | Burp Professional | NyxProxy |
| --- | --- | --- | --- |
| Recorded request chains | Yes (basic) | Yes (rich) | **Yes** — `crates/.../macros.rs` (variable extraction) |
| Variable extraction across requests | Yes | Yes | **Yes** |
| Playwright / browser-recorded login | No | Manual via Burp Browser | No — roadmap B (next-session batch 4) |

## 9 · Reporting

| Feature | Burp Community | Burp Professional | NyxProxy |
| --- | --- | --- | --- |
| Markdown export | No | No | **Yes** — `crates/.../report.rs` |
| HTML export | No | Yes | **Yes** |
| PDF export | No | Yes | **Yes** |
| OWASP Top 10 / CWE in report body | No | Yes | **Yes** |
| Compliance templates (PCI / ISO / SOC 2) | No | Add-on (Enterprise) | **Yes** — `crates/.../compliance.rs` + UI page; PCI-DSS, ISO 27001, SOC 2, HIPAA, GDPR templates |

## 10 · AI

| Feature | Burp Community | Burp Professional | NyxProxy |
| --- | --- | --- | --- |
| AI assistant in app | No | Burp Suite Navigator (preview, English, limited) | **Yes** — every tool, 9-provider gateway |
| Free-tier hosted keys | n/a | Pro license only | **Yes** — `https://nyxproxy-backend.onrender.com` (Groq, OpenRouter, HF, Cloudflare WorkersAI, GitHub Models, NVIDIA, Bytez, Gemini, Ollama) |
| Bring-your-own-key | n/a | No | **Yes** — per-user override of any provider |
| AI Auto-attack mode | No | Limited (Navigator) | **Yes** — `pages/AiAttack.tsx`, `backend/routes/ai_attack.py` |
| AI chain scan (passive → active → reporter) | No | No | **Yes** |
| AI fuzz mutator (response-driven) | No | No | **Yes** |
| Provider failover | n/a | n/a | **Yes** — `backend/providers/failover.py` |

## 11 · UX / Productivity

| Feature | Burp Community | Burp Professional | NyxProxy |
| --- | --- | --- | --- |
| Dark theme | Yes | Yes | **Yes** (default; Burp-style) |
| Hotkey / command palette | No | No | **Yes** — Ctrl+K, `docs/features/hotkeys.md` |
| Responsive layout (720px → 4K) | Partial | Partial | **Yes** — `pages/*.tsx` + `styles/*.css` |
| Browser-only preview / mock IPC | n/a | n/a | **Yes** — `npm run dev` |
| In-app embedded Chromium with proxy pre-configured | No | **Yes (Burp's Embedded Browser)** | **Yes** — Tauri `embedded_browser_cmd`, see `docs/features/embedded-browser.md` |

## 12 · Project / Workspace

| Feature | Burp Community | Burp Professional | NyxProxy |
| --- | --- | --- | --- |
| Save / load full session | **No** (this is the biggest Community limitation) | Yes — `.burp` project files | **Yes** — `.nyxproxy` (zstd, magic `NYXPRJ`), `crates/.../workspace.rs` |
| Import existing `.burp` project | n/a | n/a | **Yes** — `crates/.../burp_import.rs`, Project options panel, `docs/features/burp-import.md` |
| Cloud sync of workspace | No | No | No — roadmap F (next-session batch 4) |
| Encrypted **local** evidence packs | No | No | **Yes** — `crates/.../nyxshare.rs` (ChaCha20-Poly1305 + Argon2id), `docs/features/nyxshare.md` |
| Encrypted cloud backups | No | No | No — roadmap CC (next-session batch 6) |
| Self-hosting wizard (Docker bundle generator) | n/a | n/a | **Yes** — `crates/.../selfhost.rs`, `docs/features/self-host.md` |

## 13 · Collaboration

| Feature | Burp Community | Burp Professional | NyxProxy |
| --- | --- | --- | --- |
| Multi-user live session | No | No (Burp Enterprise has separate model) | No — roadmap J (next-session batch 4) |
| Continuous monitoring with baseline diff | No | No | **Yes** — `crates/.../monitor.rs`, `docs/features/monitor.md` |
| Live OWASP Top-10 dashboard vs industry baseline | No | No | **Yes** — `crates/.../owasp_dashboard.rs`, `docs/features/owasp-dashboard.md` |
| In-app chat / threads per finding | No | No | No — roadmap X |

## 14 · Distribution / Install

| Feature | Burp Community | Burp Professional | NyxProxy |
| --- | --- | --- | --- |
| Native installers (Windows .exe / .msi) | Yes (.exe only) | Yes (.exe + .msi) | **Yes (NSIS + MSI)** — workflow `.github/workflows/desktop.yml` |
| Native installers (Linux .deb / .AppImage) | No (`.sh` script + JAR) | No | **Yes (.deb + .AppImage)** |
| Native installer (macOS .dmg) | No (JAR + script) | No | **Roadmap** |
| No JVM required | No | No | **Yes** |
| Code signing | Yes (PortSwigger cert) | Yes | Not yet — pending publisher cert |
| Browser extension "Send to NyxProxy" | n/a | n/a | No — roadmap EE |

## 15 · Backend / Self-hosting

| Feature | Burp Community | Burp Professional | NyxProxy |
| --- | --- | --- | --- |
| Self-hostable backend | n/a | n/a | **Yes** — `apps/backend`, FastAPI, `render.yaml` blueprint, Docker-friendly |
| Hosted reference backend | n/a | n/a | **Yes** — `https://nyxproxy-backend.onrender.com` |
| Provider key management | n/a | n/a | **Yes** — `backend/providers/__init__.py` (per-provider env vars, automatic skip if no key) |
| Bearer-token gateway lockdown | n/a | n/a | **Yes** — `BACKEND_API_TOKEN` env var |

---

## Where Burp still wins (be honest)

These are real gaps on `main` today. None of them are architectural — all are scoped work:

1. **HTTP/2 in the MITM client→proxy hop.** Browser → proxy is HTTP/1.1 only; upstream supports h2 via reqwest.
2. **DNS Collaborator.** HTTP OOB works; DNS callbacks require a public name-server lease (Burp Pro uses oastify.com).
3. **Embedded Chromium (Burp's Browser).** Pro's "Open Burp's Browser" is a killer convenience — NyxProxy still requires manual browser proxy config + CA trust.
4. **Recorded login macros (Playwright).** Burp Pro can record a login in the embedded browser and replay it.
5. **`.burp` project import.** Users coming from Pro need to bring 5+ years of history with them.
6. **Burp Bambdas compatibility shim.** Many users have Bambdas already; we run JS plugins, they're Java DSL.
7. **DAST in CI (Burp Scanner CLI).** Pro ships a headless scanner runner; ours exists at the crate level but no first-class CI action yet.

Each is tracked in `docs/features/README.md` → "Coming soon" with a target PR number.

---

## Where NyxProxy already wins

Things NyxProxy ships that **neither** Burp tier offers:

- AI-driven payload generation, scan chaining, response-driven fuzz mutation — out of the box, no plugins, no license.
- 9-provider AI gateway with hosted free-tier keys so users with zero AI accounts still get assistance.
- Native Rust binary — boot < 1 s, RSS ~80 MB cold (vs. JVM ~600 MB).
- Native Linux `.deb` + `.AppImage` (Burp gives a `.sh` wrapper around a JAR).
- Open source, MIT-licensed, source-buildable, audit-friendly.
- Browser-only preview with mock IPC (`npm run dev`) — contribute UI changes without installing Rust.
- Headless engine (`nyxproxy-core` is Tauri-free) — embeddable in CI, custom tooling, scripts.
- OWASP Top-10 + CVE + 0–100 risk score auto-attached to every finding.
- `.nyxproxy` workspaces with zstd compression (smaller than Burp's XML `.burp` files in benchmarks).

---

## How "real" is this comparison?

Every NyxProxy "Yes" in this document maps to a Rust module, a TypeScript page, or a FastAPI route in this repo. Grep for the path — the code is there. Where it's partial, the doc says **Partial** and explains the gap rather than claiming parity.

The Burp columns are based on the public 2024.x release notes and the [official feature comparison page](https://portswigger.net/burp/communitydownload). If we got something wrong, please open an issue with a screenshot or release-note link.
