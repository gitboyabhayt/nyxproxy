# NyxProxy Feature Documentation

Each feature listed here has a dedicated guide explaining what it does, how to use it, and the underlying code.

## Phase 1 — Core

| Feature | Page | Doc |
|---|---|---|
| Intercepting proxy + CA | Proxy | [proxy.md](proxy.md) |
| Request history | Logger | [logger.md](logger.md) |
| Repeater | Repeater | [repeater.md](repeater.md) |
| Decoder | Decoder | [decoder.md](decoder.md) |
| **JWT toolkit** *(new in PR #4)* | Decoder → JWT tab | [jwt-toolkit.md](jwt-toolkit.md) |

## Phase 2 — Attack tools

| Feature | Page | Doc |
|---|---|---|
| Intruder | Intruder | [intruder.md](intruder.md) |
| Sequencer | Sequencer | [sequencer.md](sequencer.md) |
| Comparer | Comparer | [comparer.md](comparer.md) |

## Phase 3 — Discovery

| Feature | Page | Doc |
|---|---|---|
| Target / scope | Target | [target.md](target.md) |
| Scanner (passive + active) | Logger badges | [scanner.md](scanner.md) |
| Spider / Crawler | Target → Spider | [spider.md](spider.md) |
| Collaborator (OOB) | Collaborator | [collaborator.md](collaborator.md) |

## Phase 4 — Automation

| Feature | Page | Doc |
|---|---|---|
| Macros (request chains) | Macros | [macros.md](macros.md) |
| Plugins / Extender | Extender | [extender.md](extender.md) |

## Phase 5 — AI

| Feature | Page | Doc |
|---|---|---|
| AI Assistant | AI Assistant | [ai-assistant.md](ai-assistant.md) |
| AI analysis on requests | Logger / Repeater | [ai-analysis.md](ai-analysis.md) |
| Provider configuration | User options | [providers.md](providers.md) |
| **AI Auto-attack / Chain scan / Fuzz mutator** *(new in PR #6)* | AI Attack | [ai-attack.md](ai-attack.md) |

## Phase 6 — UX & meta

| Feature | Page | Doc |
|---|---|---|
| **WebSocket viewer & replay** *(new in PR #5)* | WebSockets | [websockets.md](websockets.md) |
| **Hotkey palette (Ctrl + K)** *(new in PR #4)* | Global | [hotkeys.md](hotkeys.md) |
| **Project workspaces** *(new in PR #4)* | Project options | [workspaces.md](workspaces.md) |
| **OWASP / risk scoring** *(new in PR #4)* | Logger badges | [risk-scoring.md](risk-scoring.md) |
| **Live OWASP Top-10 dashboard** *(new in batch 3)* | OWASP dashboard | [owasp-dashboard.md](owasp-dashboard.md) |
| **Continuous monitoring** *(new in batch 3)* | Monitor | [monitor.md](monitor.md) |
| **Self-hosting wizard** *(new in batch 3)* | Project options | [self-host.md](self-host.md) |
| **Encrypted evidence packs (`.nyxshare`)** *(new in batch 3)* | Project options | [nyxshare.md](nyxshare.md) |
| **CVE / CWE mapping** *(new in PR #4)* | Backend (`/findings/map-cve`) | [cve-mapping.md](cve-mapping.md) |
| **Recorded Playwright macros** *(new in batch 4)* | Macros → Playwright recordings | [recorded-macros.md](recorded-macros.md) |
| **Cloud sync (Supabase)** *(new in batch 4)* | User options → Cloud sync | [cloud-sync.md](cloud-sync.md) |
| **Live multi-user collaboration** *(new in batch 4)* | Live collab | [collaboration.md](collaboration.md) |
| **Distributed scan fleet** *(new in batch 4)* | Distributed scan | [distributed-scan.md](distributed-scan.md) |
| Test backend connection | User options | [backend-test.md](backend-test.md) |
| Responsive layout | Global | [responsive.md](responsive.md) |
| Installer | n/a | [../install/README.md](../install/README.md) |

## Coming soon

These features are tracked in the roadmap but not yet shipped — see `ROADMAP.md` in the repo root.

| Feature | Target PR |
|---|---|
| ~~WebSocket viewer + replay (A)~~ — **landed in PR #5** | done |
| ~~Recorded Playwright macros (B)~~ — **landed in batch 4** | done |
| ~~AI auto-attack mode (C)~~ — **landed in PR #6** | done |
| Burp `.burp` import (E) | #10 |
| ~~Cloud sync via Supabase (F)~~ — **landed in batch 4** | done |
| HTTP/2 + HTTP/3 (G) | #5 |
| ~~AI chained scanning (H)~~ — **landed in PR #6** | done |
| ~~Live multi-user (J)~~ — **landed in batch 4** | done |
| ~~Distributed scanning (K)~~ — **landed in batch 4** | done |
| ~~AI fuzz mutator (L)~~ — **landed in PR #6** | done |
| DevTools-style trace (M) | #9 |
| ~~CVE auto-mapping (N)~~ — **landed in PR #4** | done |
| NyxStore plugin marketplace (P) | #8 |
| GraphQL native (R) | #10 |
| Mobile proxy (S) | #10 |
| CI/CD GitHub Action (T) | #8 |
| Web shell sandbox (U) | #10 |
| AI-narrated PoC video (V) | #9 |
| In-app chat (X) | #7 |
| ~~Self-hosting wizard (Y)~~ — **landed in batch 3** | done |
| AI prompt marketplace (Z) | #8 |
| ~~Continuous monitoring (AA)~~ — **landed in batch 3** | done |
| OpenAPI auto-tests (BB) | #9 |
| Encrypted cloud backups (CC) | #7 |
| Embedded browser (DD) | #5 |
| Browser extension (EE) | #8 |
| mitmproxy script compat (FF) | #8 |
| Wireshark / pcap export (GG) | #8 |
| Compliance templates (II) | #9 |
| Burp Bambdas compat shim (JJ) | #8 |
