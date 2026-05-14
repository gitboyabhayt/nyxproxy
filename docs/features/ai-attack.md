# AI Attack: Auto-attack · Chained scan · Fuzz mutator

NyxProxy ships three AI-powered offensive features that talk to the hosted
gateway (or your own self-hosted backend) instead of any single LLM provider.
The gateway transparently fails over across **Groq → OpenRouter → Gemini →
GitHub Models → Cloudflare Workers AI → HuggingFace → NVIDIA → Bytez →
Ollama** so you keep getting answers even if one provider is rate-limited or
unavailable.

## Where to find it

Open the **AI Attack** entry in the left rail (next to *AI Assistant*). Three
sub-tabs:

1. **Auto-attack** — generates a ranked plan of vulnerability vectors and
   payloads for the latest captured flow.
2. **Chain scan** — runs an AI-orchestrated passive → active → report pipeline
   and produces a single risk score.
3. **Fuzz mutator** — given a seed payload, asks the AI to produce smart,
   technique-tagged mutations (case-shift, encoding swaps, polyglots, WAF
   bypass tricks).

## Auto-attack (`POST /v1/ai/auto-attack`)

Body:

```json
{
  "request": { "method": "POST", "url": "https://target/login", "body": "u=a&p=b" },
  "suspected": ["sqli", "auth_bypass"],
  "payloads_per_class": 5
}
```

Response (`AutoAttackPlan`):

```json
{
  "summary": "Login endpoint likely vulnerable to SQLi…",
  "vectors": [
    {
      "vuln": "sqli",
      "parameter": "u",
      "location": "body",
      "severity": "high",
      "payloads": [
        { "payload": "' OR 1=1--", "rationale": "auth bypass", "exploitability": 90 }
      ]
    }
  ],
  "provider": "groq",
  "model": "llama-3.3-70b-versatile",
  "fallbacks_tried": []
}
```

The backend sorts each vector's payloads by `exploitability` descending so
the most promising ones are at the top.

## Chained scan (`POST /v1/ai/chain-scan`)

Runs three logical phases in one prompt — passive (response headers, error
strings), active (suggested payloads), report (next actions + clamped 0–100
risk score). Useful as a quick "is this endpoint worth investigating?" check.

## Fuzz mutator (`POST /v1/ai/fuzz-mutate`)

Takes a single seed payload and returns up to `count` deduplicated mutations
with technique labels (`case-shift`, `tag-swap`, `nullbyte`, …) and a list of
WAFs/filters each mutation is designed to bypass.

## Provider failover

All three endpoints share `nyxproxy_backend.providers.failover.run_with_failover`.
When the chosen provider returns a transient error (`429`, `5xx`, network
failure), the backend re-issues the same prompt against the next available
provider in `DEFAULT_CHAIN`. The final response carries:

- `provider` — the provider that actually answered.
- `fallbacks_tried` — list of providers that errored before success.

If every credentialed provider fails, the API returns **HTTP 503** with the
full attempt log so the UI can show which providers were tried.

## Desktop integration

| Layer | File |
|---|---|
| Rust client | `apps/desktop/src-tauri/src/ai.rs` (`auto_attack`, `fuzz_mutate`, `chain_scan`) |
| Tauri commands | `apps/desktop/src-tauri/src/commands.rs` (`ai_auto_attack`, `ai_fuzz_mutate`, `ai_chain_scan`) |
| TS bindings | `apps/desktop/src/tauri/api.ts` (`AiApi.autoAttack`, `AiApi.fuzzMutate`, `AiApi.chainScan`) |
| UI page | `apps/desktop/src/pages/AiAttack.tsx` |
| Backend route | `apps/backend/nyxproxy_backend/routes/ai_attack.py` |
| Provider failover | `apps/backend/nyxproxy_backend/providers/failover.py` |

## Tests

Backend coverage lives in `apps/backend/tests/test_ai_attack.py` and covers:

- ranked plan parsing,
- markdown code fence stripping,
- failover on 429,
- mutation dedupe + cap,
- risk-score clamping,
- all-providers-fail → 503,
- non-JSON LLM output → 502.
