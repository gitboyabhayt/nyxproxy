# Risk scoring & OWASP categorisation

Every scanner finding is enriched with two deterministic fields:

| Field | Range | Source |
|---|---|---|
| **OWASP code** | `A01` … `A10`, `OTH` | Static rule-id → OWASP map (`nyxproxy-core::owasp::category_for_rule`) |
| **Risk score** | `0`–`100` (clamped) | Severity × confidence × OWASP-category bias (`nyxproxy-core::risk::score_issue`) |

## OWASP mapping

Every rule_id emitted by the scanner has a fixed mapping. Examples:

| Rule | OWASP code | OWASP title |
|---|---|---|
| `sql-injection`, `nosql-injection`, `xss-reflected`, `command-injection` | `A03` | Injection |
| `jwt-alg-none`, `jwt-weak-secret`, `auth-bypass`, `weak-password` | `A07` | Identification & Authentication Failures |
| `ssrf`, `ssrf-canary` | `A10` | Server-Side Request Forgery |
| `missing-security-headers`, `default-credentials`, `verbose-error` | `A05` | Security Misconfiguration |
| `vulnerable-component`, `outdated-library` | `A06` | Vulnerable and Outdated Components |
| Anything unknown | `OTH` | Other |

> The same mapping is mirrored server-side in `nyxproxy-backend.routes.findings.categorize` so third-party scanners (Burp imports, OpenAPI tests, Nuclei templates) can be enriched after the fact via `GET /findings/categorize-owasp?description=…`.

## Risk score formula

```text
base = severity_base(severity)              // info=5, low=25, medium=50, high=75, critical=95
mult = confidence_multiplier(confidence)    // tentative=0.6, firm=0.85, certain=1.0
bias = owasp_category_bias(owasp_code)      // A01/A03=+5, A10=+4, A02/A07=+3, A05=+1, others=0
score = clamp(round(base * mult + bias), 0, 100)
```

So:

| Severity | Confidence | Category | Score |
|---|---|---|---|
| Critical | Certain | A03 Injection | **100** |
| High | Firm | A10 SSRF | 68 |
| Medium | Tentative | A05 Misconfig | 31 |
| Info | Tentative | Other | 3 |

## Aggregate workspace score

`RiskApi.summary(issues)` returns the **maximum** issue score (not the sum) plus a per-OWASP bucket count + max score. This matches how Burp Pro and DefectDojo present aggregate posture — a workspace is only as safe as its worst issue.

## Where the implementation lives

* Rust: <ref_file file="/home/ubuntu/nyxproxy/apps/desktop/crates/nyxproxy-core/src/risk.rs" /> + <ref_file file="/home/ubuntu/nyxproxy/apps/desktop/crates/nyxproxy-core/src/owasp.rs" /> — 9 unit tests across both files.
* Tauri commands: `risk_score_issue_cmd`, `risk_summary_cmd`.
* TypeScript wrapper: `RiskApi` in `src/tauri/api.ts`.
* Backend mirror: `/findings/categorize-owasp` (offline, deterministic).
