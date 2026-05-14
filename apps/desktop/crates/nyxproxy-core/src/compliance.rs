//! Compliance report generator (Feature II).
//!
//! Maps NyxProxy [`Issue`]s onto five well-known control frameworks
//! and renders the result as a structured [`ComplianceReport`] plus an
//! HTML and a Markdown view.
//!
//! Supported frameworks:
//!
//! * **PCI-DSS v4.0** — focused on requirements 6.x (Secure Software)
//!   and 11.x (Test Security Regularly).
//! * **ISO/IEC 27001:2022** — Annex A controls (A.5–A.8).
//! * **SOC 2 Trust Services Criteria (2017)** — CC6.x (Logical Access)
//!   and CC7.x (System Operations).
//! * **HIPAA Security Rule** — §164.308 / §164.312 safeguards.
//! * **GDPR** — Articles 5, 25, 32.
//!
//! Mapping is **rule-based** — every `Issue` carries a category derived
//! from the scanner's rule id (e.g. `xss.reflected`, `missing-header.csp`,
//! `auth.broken`). We translate each rule id to the relevant control(s)
//! in each framework. Unmapped rules fall under a "general security
//! hygiene" bucket and are still listed in the report so nothing is
//! silently dropped.

use serde::{Deserialize, Serialize};

use crate::scanner::{Issue, IssueSeverity};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[serde(rename_all = "kebab-case")]
pub enum ComplianceFramework {
    PciDss,
    Iso27001,
    Soc2,
    Hipaa,
    Gdpr,
}

impl ComplianceFramework {
    pub fn label(self) -> &'static str {
        match self {
            ComplianceFramework::PciDss => "PCI-DSS v4.0",
            ComplianceFramework::Iso27001 => "ISO/IEC 27001:2022",
            ComplianceFramework::Soc2 => "SOC 2 (TSC 2017)",
            ComplianceFramework::Hipaa => "HIPAA Security Rule",
            ComplianceFramework::Gdpr => "GDPR (EU 2016/679)",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ControlMapping {
    pub framework: ComplianceFramework,
    pub control_id: String,
    pub control_title: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComplianceFinding {
    pub issue_name: String,
    pub severity: IssueSeverity,
    pub url: String,
    pub controls: Vec<ControlMapping>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ComplianceReport {
    pub generated_at: String,
    pub frameworks: Vec<ComplianceFramework>,
    pub findings: Vec<ComplianceFinding>,
    /// Per-framework summary: how many findings touch each control.
    pub coverage: Vec<FrameworkCoverage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrameworkCoverage {
    pub framework: ComplianceFramework,
    pub control_id: String,
    pub control_title: String,
    pub finding_count: usize,
}

/// Build a [`ComplianceReport`] for the requested frameworks.
pub fn build_report(issues: &[Issue], frameworks: &[ComplianceFramework]) -> ComplianceReport {
    let mut report = ComplianceReport {
        generated_at: chrono::Utc::now().to_rfc3339(),
        frameworks: frameworks.to_vec(),
        findings: Vec::with_capacity(issues.len()),
        coverage: Vec::new(),
    };
    let mut counter: std::collections::BTreeMap<
        (ComplianceFramework, String, String),
        usize,
    > = std::collections::BTreeMap::new();

    for issue in issues {
        let mappings = map_issue(issue, frameworks);
        for m in &mappings {
            *counter
                .entry((m.framework, m.control_id.clone(), m.control_title.clone()))
                .or_default() += 1;
        }
        let url = format!("https://{}{}", issue.host, issue.path);
        report.findings.push(ComplianceFinding {
            issue_name: issue.name.clone(),
            severity: issue.severity,
            url,
            controls: mappings,
        });
    }

    for ((framework, control_id, control_title), finding_count) in counter {
        report.coverage.push(FrameworkCoverage {
            framework,
            control_id,
            control_title,
            finding_count,
        });
    }
    report
}

/// Render a [`ComplianceReport`] as a stand-alone HTML page.
pub fn render_html(report: &ComplianceReport) -> String {
    let mut html = String::with_capacity(4096);
    html.push_str(
        r#"<!doctype html><html><head><meta charset="utf-8"><title>NyxProxy compliance report</title>
<style>
body{font-family:system-ui,sans-serif;color:#111;background:#fafafa;margin:0;padding:24px;}
h1{font-size:22px;margin:0 0 12px;} h2{font-size:18px;margin:24px 0 8px;}
table{border-collapse:collapse;width:100%;font-size:14px;margin-bottom:16px;}
th,td{padding:8px 10px;border-bottom:1px solid #ddd;text-align:left;vertical-align:top;}
th{background:#eee;}
.sev{display:inline-block;padding:2px 6px;border-radius:4px;color:#fff;font-weight:600;font-size:12px;text-transform:uppercase;}
.sev-critical{background:#a01010;} .sev-high{background:#d05a00;} .sev-medium{background:#b8a000;} .sev-low{background:#6090c0;} .sev-info{background:#888;}
.muted{color:#666;font-size:12px;}
</style></head><body>"#,
    );
    html.push_str(&format!(
        "<h1>NyxProxy compliance report</h1><div class='muted'>Generated {}</div>",
        report.generated_at
    ));
    if report.findings.is_empty() {
        html.push_str("<p>No findings — every selected framework is fully covered by the current scope.</p>");
    }

    // Per-framework coverage table.
    for framework in &report.frameworks {
        html.push_str(&format!("<h2>{} coverage</h2>", framework.label()));
        html.push_str("<table><thead><tr><th>Control</th><th>Title</th><th>Findings</th></tr></thead><tbody>");
        let mut rows: Vec<&FrameworkCoverage> =
            report.coverage.iter().filter(|c| c.framework == *framework).collect();
        rows.sort_by(|a, b| b.finding_count.cmp(&a.finding_count));
        for c in rows {
            html.push_str(&format!(
                "<tr><td><code>{}</code></td><td>{}</td><td>{}</td></tr>",
                c.control_id, c.control_title, c.finding_count
            ));
        }
        html.push_str("</tbody></table>");
    }

    // Findings table.
    html.push_str("<h2>Findings</h2><table><thead><tr><th>Severity</th><th>Issue</th><th>URL</th><th>Mapped controls</th></tr></thead><tbody>");
    for f in &report.findings {
        let sev = match f.severity {
            IssueSeverity::Critical => "sev sev-critical",
            IssueSeverity::High => "sev sev-high",
            IssueSeverity::Medium => "sev sev-medium",
            IssueSeverity::Low => "sev sev-low",
            IssueSeverity::Info => "sev sev-info",
        };
        let controls_html: String = f
            .controls
            .iter()
            .map(|m| format!("<div><b>{}</b> <code>{}</code> — {}</div>", m.framework.label(), m.control_id, m.control_title))
            .collect();
        html.push_str(&format!(
            "<tr><td><span class='{sev}'>{:?}</span></td><td>{}</td><td><code>{}</code></td><td>{}</td></tr>",
            f.severity, escape_html(&f.issue_name), escape_html(&f.url), controls_html
        ));
    }
    html.push_str("</tbody></table></body></html>");
    html
}

/// Render the report as Markdown — useful for ticket bodies and PR
/// comments.
pub fn render_markdown(report: &ComplianceReport) -> String {
    let mut md = String::with_capacity(2048);
    md.push_str("# NyxProxy compliance report\n\n");
    md.push_str(&format!("Generated: {}\n\n", report.generated_at));
    for framework in &report.frameworks {
        md.push_str(&format!("## {} coverage\n\n", framework.label()));
        md.push_str("| Control | Title | Findings |\n|---|---|---|\n");
        let mut rows: Vec<&FrameworkCoverage> =
            report.coverage.iter().filter(|c| c.framework == *framework).collect();
        rows.sort_by(|a, b| b.finding_count.cmp(&a.finding_count));
        for c in rows {
            md.push_str(&format!(
                "| `{}` | {} | {} |\n",
                c.control_id, c.control_title, c.finding_count
            ));
        }
        md.push('\n');
    }
    md.push_str("## Findings\n\n");
    md.push_str("| Severity | Issue | URL |\n|---|---|---|\n");
    for f in &report.findings {
        md.push_str(&format!(
            "| {:?} | {} | `{}` |\n",
            f.severity, f.issue_name, f.url
        ));
    }
    md
}

fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn map_issue(issue: &Issue, frameworks: &[ComplianceFramework]) -> Vec<ControlMapping> {
    let mut out = Vec::new();
    let id = issue.rule_id.to_lowercase();
    let category = categorise(&id);
    for fw in frameworks {
        if let Some((ctl_id, ctl_title)) = match (fw, category) {
            (ComplianceFramework::PciDss, IssueCategory::Injection) => Some((
                "6.2.4",
                "Software engineering techniques to prevent injection flaws",
            )),
            (ComplianceFramework::PciDss, IssueCategory::Xss) => Some((
                "6.2.4",
                "Software engineering techniques to prevent XSS / injection",
            )),
            (ComplianceFramework::PciDss, IssueCategory::AccessControl) => Some((
                "7.2.1",
                "Access to system components restricted by need-to-know",
            )),
            (ComplianceFramework::PciDss, IssueCategory::Crypto) => Some((
                "4.2.1",
                "Strong cryptography for cardholder data transmission",
            )),
            (ComplianceFramework::PciDss, IssueCategory::MissingHeader) => Some((
                "6.2.4",
                "Secure coding — protect against client-side attacks via security headers",
            )),
            (ComplianceFramework::PciDss, IssueCategory::InformationDisclosure) => Some((
                "6.4.3",
                "Public-facing web applications protected from new threats",
            )),
            (ComplianceFramework::Iso27001, IssueCategory::Injection)
            | (ComplianceFramework::Iso27001, IssueCategory::Xss) => {
                Some(("A.8.28", "Secure coding"))
            }
            (ComplianceFramework::Iso27001, IssueCategory::AccessControl) => {
                Some(("A.5.15", "Access control"))
            }
            (ComplianceFramework::Iso27001, IssueCategory::Crypto) => {
                Some(("A.8.24", "Use of cryptography"))
            }
            (ComplianceFramework::Iso27001, IssueCategory::MissingHeader) => Some((
                "A.8.9",
                "Configuration management — secure default configurations",
            )),
            (ComplianceFramework::Iso27001, IssueCategory::InformationDisclosure) => Some((
                "A.5.34",
                "Privacy and protection of personally identifiable information",
            )),
            (ComplianceFramework::Soc2, IssueCategory::Injection)
            | (ComplianceFramework::Soc2, IssueCategory::Xss) => Some((
                "CC6.6",
                "System security — protect from unauthorised inputs / threats",
            )),
            (ComplianceFramework::Soc2, IssueCategory::AccessControl) => {
                Some(("CC6.1", "Logical access security software & infrastructure"))
            }
            (ComplianceFramework::Soc2, IssueCategory::Crypto) => Some((
                "CC6.7",
                "Protect transmitted data via cryptography or other safeguards",
            )),
            (ComplianceFramework::Soc2, IssueCategory::MissingHeader) => {
                Some(("CC7.1", "Detect / mitigate threats from misconfigured systems"))
            }
            (ComplianceFramework::Soc2, IssueCategory::InformationDisclosure) => {
                Some(("CC6.7", "Restrict transmission of sensitive information"))
            }
            (ComplianceFramework::Hipaa, IssueCategory::Injection)
            | (ComplianceFramework::Hipaa, IssueCategory::Xss) => Some((
                "164.308(a)(1)(ii)(B)",
                "Risk management — implement security measures sufficient to reduce risks",
            )),
            (ComplianceFramework::Hipaa, IssueCategory::AccessControl) => {
                Some(("164.312(a)", "Access control to ePHI"))
            }
            (ComplianceFramework::Hipaa, IssueCategory::Crypto) => {
                Some(("164.312(e)(2)(ii)", "Encryption of ePHI in transit"))
            }
            (ComplianceFramework::Hipaa, IssueCategory::MissingHeader) => Some((
                "164.308(a)(5)(ii)(B)",
                "Protection from malicious software (defence-in-depth)",
            )),
            (ComplianceFramework::Hipaa, IssueCategory::InformationDisclosure) => {
                Some(("164.502(a)", "Uses and disclosures of ePHI"))
            }
            (ComplianceFramework::Gdpr, IssueCategory::Injection)
            | (ComplianceFramework::Gdpr, IssueCategory::Xss) => Some((
                "Article 32(1)(b)",
                "Ability to ensure ongoing confidentiality, integrity, and resilience",
            )),
            (ComplianceFramework::Gdpr, IssueCategory::AccessControl) => {
                Some(("Article 32(1)(b)", "Access control — pseudonymisation and confidentiality"))
            }
            (ComplianceFramework::Gdpr, IssueCategory::Crypto) => {
                Some(("Article 32(1)(a)", "Encryption of personal data"))
            }
            (ComplianceFramework::Gdpr, IssueCategory::MissingHeader) => Some((
                "Article 25",
                "Data protection by design and by default",
            )),
            (ComplianceFramework::Gdpr, IssueCategory::InformationDisclosure) => {
                Some(("Article 5(1)(f)", "Integrity and confidentiality of personal data"))
            }
            // Unknown / generic — still map to a generic control so the
            // finding shows up rather than being silently dropped.
            (ComplianceFramework::PciDss, IssueCategory::Unknown) => {
                Some(("11.4.1", "Penetration testing methodology"))
            }
            (ComplianceFramework::Iso27001, IssueCategory::Unknown) => {
                Some(("A.5.7", "Threat intelligence"))
            }
            (ComplianceFramework::Soc2, IssueCategory::Unknown) => {
                Some(("CC7.2", "Monitor system components and operation"))
            }
            (ComplianceFramework::Hipaa, IssueCategory::Unknown) => {
                Some(("164.308(a)(8)", "Evaluation — periodic technical security review"))
            }
            (ComplianceFramework::Gdpr, IssueCategory::Unknown) => {
                Some(("Article 32", "Security of processing — general"))
            }
        } {
            out.push(ControlMapping {
                framework: *fw,
                control_id: ctl_id.to_string(),
                control_title: ctl_title.to_string(),
            });
        }
    }
    out
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum IssueCategory {
    Injection,
    Xss,
    AccessControl,
    Crypto,
    MissingHeader,
    InformationDisclosure,
    Unknown,
}

fn categorise(rule_id: &str) -> IssueCategory {
    let id = rule_id.to_lowercase();
    if id.contains("xss") {
        return IssueCategory::Xss;
    }
    if id.contains("sqli")
        || id.contains("sql-injection")
        || id.contains("ssrf")
        || id.contains("path-traversal")
        || id.contains("command-injection")
        || id.contains("template-injection")
    {
        return IssueCategory::Injection;
    }
    if id.contains("auth")
        || id.contains("idor")
        || id.contains("authz")
        || id.contains("permission")
        || id.contains("access-control")
    {
        return IssueCategory::AccessControl;
    }
    if id.contains("tls")
        || id.contains("ssl")
        || id.contains("cert")
        || id.contains("jwt")
        || id.contains("crypto")
    {
        return IssueCategory::Crypto;
    }
    if id.contains("header") || id.contains("missing-") || id.contains("csp") || id.contains("hsts") {
        return IssueCategory::MissingHeader;
    }
    if id.contains("leak")
        || id.contains("disclosure")
        || id.contains("pii")
        || id.contains("error-message")
    {
        return IssueCategory::InformationDisclosure;
    }
    IssueCategory::Unknown
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scanner::{Issue, IssueConfidence, IssueSeverity};

    fn issue(rule_id: &str, sev: IssueSeverity) -> Issue {
        Issue {
            id: uuid::Uuid::new_v4().to_string(),
            flow_id: uuid::Uuid::new_v4().to_string(),
            rule_id: rule_id.into(),
            name: rule_id.replace('.', " "),
            severity: sev,
            confidence: IssueConfidence::Firm,
            description: String::new(),
            evidence: None,
            remediation: None,
            host: "example.com".into(),
            path: "/x".into(),
        }
    }

    #[test]
    fn maps_xss_finding_to_every_framework() {
        let issues = vec![issue("xss.reflected", IssueSeverity::High)];
        let report = build_report(
            &issues,
            &[
                ComplianceFramework::PciDss,
                ComplianceFramework::Iso27001,
                ComplianceFramework::Soc2,
                ComplianceFramework::Hipaa,
                ComplianceFramework::Gdpr,
            ],
        );
        assert_eq!(report.findings.len(), 1);
        let controls = &report.findings[0].controls;
        assert_eq!(controls.len(), 5);
        assert!(controls.iter().any(|c| c.control_id == "6.2.4"));
        assert!(controls.iter().any(|c| c.control_id == "A.8.28"));
        assert!(controls.iter().any(|c| c.control_id == "CC6.6"));
        assert!(controls.iter().any(|c| c.control_id == "164.308(a)(1)(ii)(B)"));
        assert!(controls.iter().any(|c| c.control_id == "Article 32(1)(b)"));
    }

    #[test]
    fn unknown_rule_still_maps_to_generic_control() {
        let issues = vec![issue("weird.unmapped.rule", IssueSeverity::Low)];
        let report = build_report(&issues, &[ComplianceFramework::PciDss]);
        assert_eq!(report.findings.len(), 1);
        assert_eq!(report.findings[0].controls.len(), 1);
        assert_eq!(report.findings[0].controls[0].control_id, "11.4.1");
    }

    #[test]
    fn coverage_counts_findings_per_control() {
        let issues = vec![
            issue("xss.reflected", IssueSeverity::High),
            issue("xss.stored", IssueSeverity::High),
            issue("tls.weak", IssueSeverity::Medium),
        ];
        let report = build_report(
            &issues,
            &[ComplianceFramework::PciDss, ComplianceFramework::Iso27001],
        );
        let xss_pci = report
            .coverage
            .iter()
            .find(|c| c.framework == ComplianceFramework::PciDss && c.control_id == "6.2.4")
            .unwrap();
        assert_eq!(xss_pci.finding_count, 2);
        let xss_iso = report
            .coverage
            .iter()
            .find(|c| c.framework == ComplianceFramework::Iso27001 && c.control_id == "A.8.28")
            .unwrap();
        assert_eq!(xss_iso.finding_count, 2);
    }

    #[test]
    fn html_render_contains_all_findings() {
        let issues = vec![issue("xss.reflected", IssueSeverity::High)];
        let report = build_report(&issues, &[ComplianceFramework::PciDss]);
        let html = render_html(&report);
        assert!(html.contains("xss reflected"));
        assert!(html.contains("6.2.4"));
        assert!(html.contains("PCI-DSS"));
    }

    #[test]
    fn markdown_render_has_one_row_per_finding() {
        let issues = vec![
            issue("xss.reflected", IssueSeverity::High),
            issue("idor.numeric", IssueSeverity::Medium),
        ];
        let md = render_markdown(&report_for(&issues));
        let lines: Vec<&str> = md.lines().filter(|l| l.starts_with("| High")
            || l.starts_with("| Medium")).collect();
        assert_eq!(lines.len(), 2);
    }

    fn report_for(issues: &[Issue]) -> ComplianceReport {
        build_report(issues, &[ComplianceFramework::PciDss])
    }
}
