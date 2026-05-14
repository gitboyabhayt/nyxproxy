# Six Years Ahead — Features That Leapfrog Burp Suite

> This is a curated short-list of features that would put NyxProxy substantially ahead of Burp Suite Professional — not just at parity. Each one is a multi-PR effort and is **proposed**, not yet implemented. They are ordered by *leverage per engineering week*.

The shipped feature roadmap (A–JJ) is in [`docs/features/README.md`](../features/README.md). The list below is the *next-tier* picks beyond that.

---

## 1. Agentic Red-Team Mode (`agent`)

An autonomous LLM-driven agent that operates the full NyxProxy toolset against a scoped target — running spider, scanner, intruder, repeater, JWT toolkit, and the OOB collaborator in sequence, deciding what to do next based on observed responses.

**Why it leapfrogs Burp.** Burp Suite Navigator (their AI preview) is reactive — it explains a finding when you click. This is *proactive*: the agent plans a 30-minute engagement, executes it, and hands you a report. Imagine "Recon a target like a senior pentester would" as a button.

**What ships.**
- `apps/backend/nyxproxy_backend/agent/` — planner (LLM), tool router (FastAPI), step recorder.
- `apps/desktop/src/pages/Agent.tsx` — live transcript of the agent's reasoning + tool calls.
- A `--agent` CLI flag on the headless `nyxproxy-core` binary for CI runs.
- Hard sandbox: agent can only call tools against in-scope hosts; out-of-scope = hard error.

**Open questions.** Token cost per run; whether to default to local Ollama for the planner to keep cost predictable.

---

## 2. Replay-aware diff fuzzing

Most fuzzers send random payloads and grep responses. We propose a fuzzer that *replays* a baseline transaction (login → action → logout), mutates a single parameter, and surfaces the *semantic* diff between baseline and mutated runs — not just an HTTP status code change.

**Why it leapfrogs Burp.** Burp Intruder is stateless per request. Real bugs (IDOR, race conditions, business-logic flaws) live in multi-request flows. A replay-aware fuzzer finds those.

**What ships.**
- `crates/nyxproxy-core/src/fuzz/replay.rs` — takes a macro + a marker, replays N times with mutated marker.
- Diff engine: response-body AST diff (HTML / JSON / XML aware), not naive line diff.
- "Anomaly score" per mutation, ranked.

---

## 3. Causal request graph (post-spider)

After the spider runs, automatically build a **causal graph** of which endpoints set which session/cookie/state used by which other endpoints. Render as an interactive graph in the Target page.

**Why it leapfrogs Burp.** Burp's sitemap is a tree of URLs. It tells you *what exists*; not *what depends on what*. The graph version reveals authentication boundaries, token lifecycle, and chained-IDOR opportunities at a glance.

**What ships.**
- `crates/nyxproxy-core/src/graph.rs` — builds the directed graph from the history store.
- `apps/desktop/src/pages/Target.tsx` — graph view tab (force-directed layout via `@dagrejs/dagre` or d3).
- AI overlay: hover any edge → LLM explains the relationship.

---

## 4. Time-travel debugging for HTTP

A scrubber bar across the Logger page. Drag it backwards and the entire app state (history, scope, scanner queue, intruder attempts) rewinds to that point. Drag forward — re-execute, optionally with a parameter changed. Branch a timeline.

**Why it leapfrogs Burp.** Burp's history is append-only. If you tested something 200 requests ago and want to redo it with a small change, you have to rebuild context manually. Time-travel makes "what if I'd sent this 30 seconds earlier" a one-drag operation.

**What ships.**
- Event-sourcing the `HistoryStore` (already JSONL on disk — minor refactor).
- A scrubber UI in the title bar.
- Branch viewer — "main timeline" + "branch from t=12:34" tabs.

---

## 5. Self-healing recorded macros

Recorded login flows (the Playwright macros from roadmap item B) break the moment the target's UI changes — even a button-text change. We propose a self-healing layer: on failure, screenshot + DOM dump get sent to the AI gateway with the original step description, and the AI proposes a new selector. Macro author accepts/rejects with one click.

**Why it leapfrogs Burp.** Burp's macros are HTTP-only request chains; they don't drive a browser. Even when *they* break, you fix them by re-recording. Self-healing means macros that survive 6 months of target UI churn.

**What ships.**
- `apps/backend/nyxproxy_backend/routes/macros_heal.py` — accepts a screenshot + failing step + history.
- LLM returns a JSON patch to the macro step.
- Desktop UI shows side-by-side "old selector vs proposed selector".

---

## 6. Live OWASP Top-10 / API Top-10 dashboard with target-specific delta

Every finding is already tagged with OWASP / CWE / risk score (shipped in PR #4). The next step: a *per-target* dashboard that shows your top-10 distribution **and** the historical distribution for similar targets (banks, e-commerce, healthcare, dev tools) sourced from public bug-bounty disclosures. Highlights anomalies — "you found 4× the industry rate of A03 Injection on this target."

**Why it leapfrogs Burp.** Burp reports are static and target-agnostic. NyxProxy's dashboard would tell you *where to look next* based on what similar targets have historically been vulnerable to.

**What ships.**
- `apps/backend/nyxproxy_backend/routes/benchmarks.py` — serves industry-aggregated OWASP distributions.
- `apps/desktop/src/pages/Dashboard.tsx` — radar chart (target vs industry baseline).
- Data source: aggregated HackerOne / Bugcrowd disclosures + manual curation, refreshed nightly.

---

## 7. SBOM-aware vulnerability surfacing

Every captured response is mined for tells: `Server:` headers, JS framework fingerprints in body, source-map references, `package.json` accidentally exposed. Build an SBOM of the target's stack, then cross-reference with NVD / OSV.dev to surface known CVEs in the discovered components — automatically, before the scanner even runs.

**Why it leapfrogs Burp.** Burp has Software Vulnerability Scanner (SVS) extension but it's a separate paid add-on. This would ship in core, run passively as traffic flows in, and *predict* which scanner checks will hit.

**What ships.**
- `crates/nyxproxy-core/src/sbom.rs` — fingerprint extraction.
- `apps/backend/nyxproxy_backend/routes/sbom.py` — proxies NVD / OSV.dev queries (cached).
- Target page → SBOM tab with CVE-tagged components.

---

## 8. Encrypted, end-to-end shareable evidence packs

Click "Share finding" on any issue → produces a self-contained `.nyxshare` file (zstd, AES-256-GCM, key in URL fragment) containing the offending request, response, repro macro, screenshots, AI-generated PoC writeup. URL fragment never hits the server. Recipient opens it in NyxProxy → full repro in 1 click. Optional expiry timestamp.

**Why it leapfrogs Burp.** Burp evidence-sharing is "paste the request into the report". This is "1-click 1-link reproducible finding" — a real workflow improvement for bug-bounty hunters and pentest reporting.

**What ships.**
- `crates/nyxproxy-core/src/evidence.rs` — pack/unpack with AES-GCM.
- A static, single-page viewer hosted on the backend (`nyxshare.html`) that decrypts in-browser using the URL fragment.
- "Import .nyxshare" entry on the Welcome page.

---

## Recommended sequence

If we could only build three of these in the next quarter, my pick:

1. **(#1) Agentic Red-Team Mode** — the headline feature; demos in 30 seconds; nothing else competes with it.
2. **(#5) Self-healing macros** — solves a real pain point Burp users complain about constantly.
3. **(#7) SBOM-aware vulns** — gives NyxProxy a foothold in the AppSec / SCA category, not just DAST.

Then circle back to **(#3) Causal graph** and **(#4) Time-travel** for the polish push.

---

## Why not [`X`]?

A few features I considered and rejected for the leapfrog list:

- **Distributed scanning fleet.** Already planned (roadmap K). Useful but not differentiating.
- **GraphQL native support.** Already planned (roadmap R). Necessary, not surprising.
- **Mobile proxy.** Already planned (roadmap S). Catch-up feature.
- **In-app team chat.** Already planned (roadmap X). Better solved by Slack/Discord integrations.
- **Compliance report generator.** Already planned (roadmap II). Boring but needed; doesn't leapfrog.

The eight above are specifically chosen because **Burp Pro does not have them, and they're not on Burp's public roadmap either** as of May 2026.
