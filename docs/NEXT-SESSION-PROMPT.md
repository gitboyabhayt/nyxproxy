# NyxProxy — implementation prompt for the next Devin session

Paste this into a new Devin session to continue the Burp-Suite-parity work
from where this session ended.

## Context for the next Devin

The repo `https://github.com/gitboyabhayt/nyxproxy.git` is a Rust/Tauri
desktop app + React frontend + FastAPI backend (deployed at
`https://nyxproxy-backend.onrender.com`). Goal is to ship a production
Burp Suite replacement, then surpass Burp Pro on differentiators.

**Already shipped on `main` (do not re-implement):**

| Letter | Feature | Code path |
|---|---|---|
| A | WebSocket viewer + replay | `apps/desktop/crates/nyxproxy-core/src/websocket.rs` |
| C | AI auto-attack mode | `apps/backend/routes/ai_attack.py` |
| D | Project workspaces (`.nyxproxy`) | `apps/desktop/crates/nyxproxy-core/src/workspace.rs` |
| E | Burp `.burp` / "Save items" XML import | `apps/desktop/crates/nyxproxy-core/src/burp_import.rs` |
| G | HTTP/2 in MITM hop (ALPN + Alt-Svc strip) | `apps/desktop/crates/nyxproxy-core/src/proxy.rs` |
| H | AI chained scanning | `apps/backend/routes/ai_attack.py::chain_scan` |
| I | Hotkey palette (Ctrl+K) | frontend `CommandPalette.tsx` + `hotkeys.md` |
| L | AI fuzz mutator | `apps/backend/routes/ai_attack.py::fuzz_mutate` |
| N | CVE / CWE mapping | `apps/backend/routes/findings.py::map_cve` |
| O | OWASP 2021 categorisation | `apps/desktop/crates/nyxproxy-core/src/owasp.rs` |
| Q | JWT toolkit | `apps/desktop/crates/nyxproxy-core/src/jwt.rs` |
| R | GraphQL native support | `apps/desktop/crates/nyxproxy-core/src/graphql.rs` |
| T | CI/CD GitHub Action | `action/action.yml`, `apps/desktop/crates/nyxproxy-scan` |
| BB | OpenAPI / Swagger auto-tests | `apps/desktop/crates/nyxproxy-core/src/openapi.rs` |
| DD | Embedded Chromium browser | Tauri `embedded_browser_cmd` |
| EE | "Send to NyxProxy" browser extension | `extension/` + `apps/desktop/.../bridge.rs` |
| GG | Wireshark / pcap export | `apps/desktop/crates/nyxproxy-core/src/pcap.rs` |
| HH | Risk scoring engine | `apps/desktop/crates/nyxproxy-core/src/risk.rs` |
| II | Compliance report generator | `apps/desktop/crates/nyxproxy-core/src/compliance.rs` |
| Y | Self-hosting wizard (batch 3) | `apps/desktop/crates/nyxproxy-core/src/selfhost.rs` |
| AA | Continuous monitoring (batch 3) | `apps/desktop/crates/nyxproxy-core/src/monitor.rs` |
| Leapfrog #6 | Live OWASP dashboard (batch 3) | `apps/desktop/crates/nyxproxy-core/src/owasp_dashboard.rs` |
| Leapfrog #8 | Encrypted `.nyxshare` evidence packs (batch 3) | `apps/desktop/crates/nyxproxy-core/src/nyxshare.rs` |

## Pending features (20 — to be implemented in this next session)

Implement these **fullstack, real, tested** — Rust module + Tauri command +
typed frontend API + UI surface + Markdown doc + unit tests. **No mocks, no
placeholders.** Group into PR batches of 4 features each (5 batches total).

### Batch 4

1. **B — Recorded Playwright login macros**
   - Save Playwright trace to `~/.nyxproxy/macros/<name>/trace.zip`.
   - New Tauri command `macro_record_start_cmd` spawns `playwright codegen`
     pointed at the chosen URL through the NyxProxy listener.
   - On stop, parse the resulting `.spec.ts` into a JSON DSL stored next to
     the trace. Trigger replay via existing `MacroStore`.
   - Tests: parser unit tests covering navigation, click, fill, expect.
   - Doc: `docs/features/recorded-macros.md`.

2. **F — Cloud sync via Supabase (optional)**
   - Backend gets `/sync/push`, `/sync/pull` routes using Supabase service
     role key (env var only). Tables: `nyx_history`, `nyx_issues`,
     `nyx_scope`, all keyed by `user_id`.
   - Tauri commands `sync_push_cmd` and `sync_pull_cmd` with conflict
     resolution `last_write_wins`.
   - User settings panel toggles sync; if backend env is missing return
     `feature disabled — set SUPABASE_URL + SUPABASE_KEY`.
   - Tests: round-trip serialisation, conflict resolver.

3. **J — Live multi-user collaboration via WebRTC**
   - Use simple-peer over backend signalling room
     `/collab/room/{room_id}`. Each peer mirrors its local
     `proxy_event_stream` to other peers.
   - Frontend `Collab` page shows live cursor + selection in Logger.
   - Tests: signalling room state machine + serialisation of cursor events.

4. **K — Distributed scanning fleet**
   - New `nyxproxy-worker` binary (cargo workspace member) that long-polls
     a backend queue endpoint `/scan/jobs/next`.
   - Backend stores jobs in a sqlite or Supabase table.
   - Tauri command `scan_distribute_cmd` shards a target list across
     registered workers.
   - Tests: shard-balancing algorithm.

### Batch 5

5. **M — Browser DevTools-style trace**
   - For each request, capture (browser-side via the extension or
     embedded browser): console logs, network waterfall, cookies,
     localStorage diff, DOM diff (after JS settles).
   - Store as `Vec<TraceEntry>` attached to `HttpFlow`.
   - New UI tab inside Logger detail.
   - Tests: trace builder + DOM diff sanity.

6. **P — NyxStore plugin marketplace**
   - Backend route `/store/plugins` returns a manifest of published
     plugins (initially seeded with first-party ones).
   - Tauri command `plugin_install_cmd` downloads + verifies signature
     (ed25519) and copies into `~/.nyxproxy/plugins/`.
   - New `Marketplace` page with search + install button.
   - Tests: manifest schema + signature verification.

7. **S — Mobile proxy mode (Android via ADB + iOS via PAC)**
   - New module `mobile.rs` wraps `adb` + `usbmuxd` (libimobiledevice).
   - On enable: install NyxProxy CA, configure WiFi/PAC proxy on the
     device automatically.
   - Tauri commands: `mobile_detect_cmd`, `mobile_provision_cmd`.
   - Tests: command builder + CA installer planner.

8. **U — Web shell sandbox**
   - For findings with `rule_id="rce-*"`, expose a sandboxed terminal
     that re-uses the captured request as the exec primitive.
   - Backend route `/exec/shell` proxies via the original endpoint with
     user-typed commands.
   - Frontend uses `xterm.js` already shipped via npm.
   - Tests: command quoting + safe-by-default toggle.

### Batch 6

9. **V — AI-narrated PoC video**
   - Backend route `/findings/{id}/poc-video` generates a `.webm` with
     `puppeteer` recording the repro + a TTS audio track (use the
     Coqui-TTS docker image or OpenAI `tts-1`).
   - Tauri command `poc_video_render_cmd` downloads the result.
   - Tests: cassette-style test that mocks the TTS provider and asserts
     the video manifest.

10. **X — In-app team chat**
    - Backend route `/chat/ws/{room}` over websockets, persistence in
      sqlite.
    - Frontend Slack-style thread per finding; uses existing AI provider
      for `/summarise` and `/translate`.
    - Tests: chat history ordering + room ACL.

11. **Z — AI prompt marketplace**
    - Backend table `prompts` keyed by `(slug, version)`.
    - Tauri command `prompt_install_cmd` fetches + stores under
      `~/.nyxproxy/prompts/`.
    - Frontend `Prompt store` page with category filter (CISO,
      OWASP, exploit-dev, write-up).
    - Tests: prompt manifest parser + version pinning.

12. **CC — Encrypted cloud backups (S3/B2/R2)**
    - Reuse `.nyxshare` cipher for at-rest encryption.
    - Tauri command `backup_push_cmd` uploads via `aws-sdk-s3` (works
      with any S3-compatible endpoint).
    - User settings: endpoint, bucket, access-key, secret.
    - Tests: round-trip with the `localstack` test container.

### Batch 7

13. **FF — mitmproxy script compatibility**
    - Embed PyO3 inline interpreter inside a Tauri command.
    - Load `.py` files that implement `request`, `response`, `websocket_message`
      hooks. Wrap our `HttpFlow` into a mitmproxy-shaped `flow` object.
    - Tests: shim against the mitmproxy "addons examples" suite.

14. **JJ — Burp Bambdas compatibility shim**
    - Embed a minimal Java DSL evaluator (use `rhino` for JS-style
      execution or transpile common Bambdas to JS at install time).
    - Hook into Proxy → Match & Replace + Intruder.
    - Tests: known-good Bambdas from the PortSwigger public examples
      pack.

15. **Leapfrog #1 — Agentic Red-Team Mode**
    - New `agent.rs` orchestrates a tool-use loop with the configured
      LLM (already in backend). Tools: `repeater_send`, `scanner_scan`,
      `intruder_launch`, `report_finding`, `request_credential`.
    - Hard stop conditions: `max_steps`, `max_cost`, `scope_violations`.
    - Tauri command `agent_run_cmd` streams trace events.
    - Tests: deterministic mock LLM that drives the loop end-to-end.

16. **Leapfrog #2 — Replay-aware diff fuzzing**
    - Multi-step macro replay where the fuzzer mutates one parameter at
      a time across the entire chain, then diffs responses.
    - Catches IDOR / race conditions Burp Intruder cannot.
    - Frontend: new Intruder mode "Chain".
    - Tests: round-trip diff against a known IDOR fixture.

### Batch 8

17. **Leapfrog #3 — Causal request graph**
    - From history, build a graph of "endpoint A's response body is
      embedded in endpoint B's request". Edges = dependencies.
    - Frontend: `Graph` page renders via `react-flow`.
    - Tests: graph builder against a known auth-token flow.

18. **Leapfrog #4 — Time-travel HTTP debugging**
    - Treat history as an immutable event log with a scrubber bar:
      pick any point in time and the rest of the UI reflects state at
      that instant.
    - Branch timelines: re-run from a previous point with edited
      parameters, attached as a sibling history.
    - Tests: state-machine snapshot tests.

19. **Leapfrog #5 — Self-healing recorded macros**
    - When a Playwright macro step fails, send the failing selector +
      current DOM to the AI provider, which returns a new selector;
      retry once, persist the new selector.
    - Tests: deterministic mock provider that returns a fixed correction.

20. **Leapfrog #7 — SBOM-aware vulnerability surfacing**
    - On any captured `Server`, `X-Powered-By`, or response body
      revealing a framework version → look up CVEs via the
      already-implemented `findings/map-cve` route.
    - Surface as passive scanner finding with "exploit available" badge
      where Exploit-DB lists one.
    - Tests: fixture-based SBOM detection across Nginx / Express /
      Drupal banners.

## Standing implementation rules (the prior session set these)

- **No mocks, no placeholders.** If a feature requires a real subsystem,
  build it. If a credential is missing, request it from the user via
  `request_secret`.
- Every feature gets: Rust module + unit tests + Tauri command +
  registered command in `apps/desktop/src-tauri/src/lib.rs` + typed
  frontend wrapper in `apps/desktop/src/tauri/api.ts` + a React UI
  surface + a markdown doc in `docs/features/`.
- Every batch (4 features) lands as **one PR** branched off `main`. Wait
  for CI green before opening the next batch.
- Add to the existing browser-preview fallback in `api.ts` so the dev
  build keeps working without Tauri.
- Run `cargo test -p nyxproxy-core --lib --tests`, `cargo check
  -p nyxproxy-app`, `npm run typecheck`, `npm run build` before pushing.
- Don't touch `main` directly. Branch name pattern:
  `devin/<unix-ts>-batch<N>-features`.
- Use the saved `GITHUB_PAT_NYXPROXY` secret. If push 403s, instruct
  the user to extend the token's `Contents: write` + `Workflows: write`
  permissions on `gitboyabhayt/nyxproxy`.

## Suggested batch order (for next session)

Batch 4 → Batch 5 → Batch 6 → Batch 7 → Batch 8 in that order. Pause
after each PR for the user's review, then continue.

## Final deliverable for next session

After all 20 features land:

- Update `docs/burp-vs-nyxproxy.md` with the final, honest comparison
  (every "Partial" should now be "Yes" except for HTTP/3 in MITM).
- Update `docs/leapfrog-features.md` to mark all 8 leapfrog features as
  shipped.
- Update `README.md` with the new feature matrix.
- Open one final PR titled "docs: final NyxProxy vs Burp comparison after
  full feature parity".
