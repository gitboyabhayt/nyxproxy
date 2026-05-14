# Live OWASP Top-10 dashboard + industry baseline delta (Leapfrog #6)

A real-time view of the OWASP 2021 Top-10 distribution of the current
issue queue, with a delta against the published industry prevalence
baseline so you can see whether your engagement is **over-** or
**under-represented** in each category.

## Categories

The dashboard renders all ten OWASP 2021 categories regardless of count so
the user can see the shape of "what's missing" too:

```
A01 Broken Access Control
A02 Cryptographic Failures
A03 Injection
A04 Insecure Design
A05 Security Misconfiguration
A06 Vulnerable & Outdated Components
A07 Identification & Authentication Failures
A08 Software & Data Integrity Failures
A09 Security Logging & Monitoring Failures
A10 Server-Side Request Forgery (SSRF)
```

Rules without an OWASP mapping (`category_for_rule` returns `None`) are
counted in an `UNK` "Unmapped" pseudo-category.

## Computation

For each category:

```text
count        = number of issues with that category
percent      = count / total * 100
baseline     = published industry prevalence (hard-coded in
                `owasp_dashboard::BASELINE`)
delta_pp     = percent - baseline    (percentage points)
```

A positive delta means *over-represented* (you found this class **more**
than industry average). Negative means under-represented or the
engagement scope doesn't touch this class.

## API

| Tauri command            | Purpose                                            |
| ------------------------ | -------------------------------------------------- |
| `owasp_dashboard_cmd`    | Build a dashboard struct from a list of `Issue`s.  |

## UI

Sidebar → **OWASP dashboard**. The page loads the current
`scanner_scan_history`, builds the dashboard, and renders:

- A four-column distribution table (`Count`, `You %`, `Industry %`, `Δ pp`).
- A horizontal progress bar per category for the at-a-glance comparison.

## Tests

`apps/desktop/crates/nyxproxy-core/src/owasp_dashboard.rs` ships four
tests: empty-dashboard shape, percent-accounting, over-represented delta,
unknown-rule bucket fallback.
