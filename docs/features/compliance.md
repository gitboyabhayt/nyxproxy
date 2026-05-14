# Compliance reports (Feature II)

NyxProxy maps scanner findings to five well-known control frameworks
and renders the result as a structured `ComplianceReport`, an HTML
page, and a Markdown table.

## Supported frameworks

| Framework | Coverage focus |
|---|---|
| **PCI-DSS v4.0** | Requirements 4.x (Transmission), 6.x (Secure Software), 7.x (Access), 11.x (Test Regularly). |
| **ISO/IEC 27001:2022** | Annex A controls A.5 (Org), A.8 (Tech). |
| **SOC 2 (TSC 2017)** | CC6.x (Logical Access), CC7.x (System Operations). |
| **HIPAA Security Rule** | §164.308 / §164.312 safeguards. |
| **GDPR (EU 2016/679)** | Articles 5, 25, 32. |

## Mapping

Mapping is **rule-based**. Every scanner finding carries a category
derived from its rule id (e.g. `xss.reflected`, `missing-header.csp`,
`auth.broken`, `tls.weak`). Each rule id is translated to the relevant
control(s) per framework. Unknown rule ids still map to a generic
control so the finding is never silently dropped.

Examples:

| Rule category | PCI-DSS | ISO 27001 | SOC 2 | HIPAA | GDPR |
|---|---|---|---|---|---|
| `xss.*`, `sqli.*`, injection | 6.2.4 | A.8.28 | CC6.6 | 164.308(a)(1)(ii)(B) | Article 32(1)(b) |
| `auth.*`, `idor.*`, access control | 7.2.1 | A.5.15 | CC6.1 | 164.312(a) | Article 32(1)(b) |
| `tls.*`, `jwt.*`, crypto | 4.2.1 | A.8.24 | CC6.7 | 164.312(e)(2)(ii) | Article 32(1)(a) |
| `missing-header.*`, `csp` | 6.2.4 | A.8.9 | CC7.1 | 164.308(a)(5)(ii)(B) | Article 25 |
| `info-disclosure`, `pii` | 6.4.3 | A.5.34 | CC6.7 | 164.502(a) | Article 5(1)(f) |

## UI

Open **Compliance** in the sidebar:

1. Pick the frameworks you care about (multi-select).
2. Press *Build report*.
3. *View HTML* opens a stand-alone page (browser tab) with severity
   colour-coding and per-framework coverage tables.
4. *Copy Markdown* puts a Markdown report on your clipboard, ready
   for a ticket body or PR comment.

## Programmatic access

```rust
use nyxproxy_core::compliance::{
    build_report, render_html, render_markdown, ComplianceFramework,
};

let report = build_report(&issues, &[
    ComplianceFramework::PciDss,
    ComplianceFramework::Iso27001,
]);
let html = render_html(&report);
let md = render_markdown(&report);
```

## Tested

```
cargo test -p nyxproxy-core --lib compliance
```

5 tests:

- `maps_xss_finding_to_every_framework` — one XSS finding produces a
  mapping in every selected framework.
- `unknown_rule_still_maps_to_generic_control` — unknown rule ids
  fall back to a generic control.
- `coverage_counts_findings_per_control` — 2 XSS findings show
  `finding_count = 2` in both PCI 6.2.4 and ISO A.8.28.
- `html_render_contains_all_findings` — HTML render includes the
  finding name and control IDs.
- `markdown_render_has_one_row_per_finding` — Markdown table has one
  row per finding, no fewer.
