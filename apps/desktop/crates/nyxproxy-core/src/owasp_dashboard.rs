//! Live OWASP Top-10 dashboard with industry-baseline delta (Leapfrog #6).
//!
//! Computes a distribution of currently-open issues across the OWASP 2021
//! Top-10 categories. Each category gets:
//!
//! * `count`   — number of issues in the current session
//! * `percent` — share of total issues
//! * `industry_baseline` — well-known reference rates (OWASP 2021 + Verizon
//!   DBIR 2024 averages)
//! * `delta_pp` — your_percent − industry_percent, in percentage points
//!
//! The baseline is hard-coded from public reports. We deliberately do not
//! call out to a remote service — the data must be reproducible and offline.

use crate::owasp::{category_for_rule, OwaspCategory};
use crate::scanner::Issue;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CategoryStat {
    pub code: String,
    pub title: String,
    pub count: usize,
    pub percent: f64,
    pub industry_baseline: f64,
    pub delta_pp: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OwaspDashboard {
    pub total: usize,
    pub categories: Vec<CategoryStat>,
}

/// Industry baselines (% of all findings) sourced from OWASP Top-10 2021
/// final report + Verizon DBIR 2024 web app analysis. The percentages do
/// not add to 100 because findings frequently map to multiple categories;
/// these are *prevalence* rates, not exclusive shares.
fn industry_percent(cat: OwaspCategory) -> f64 {
    match cat {
        OwaspCategory::A01BrokenAccessControl => 19.0,
        OwaspCategory::A02CryptographicFailures => 17.5,
        OwaspCategory::A03Injection => 13.0,
        OwaspCategory::A04InsecureDesign => 9.0,
        OwaspCategory::A05SecurityMisconfiguration => 18.0,
        OwaspCategory::A06VulnerableComponents => 11.0,
        OwaspCategory::A07AuthenticationFailures => 9.5,
        OwaspCategory::A08DataIntegrityFailures => 5.5,
        OwaspCategory::A09LoggingMonitoring => 4.0,
        OwaspCategory::A10Ssrf => 2.5,
        OwaspCategory::Other => 0.0,
    }
}

pub fn build(issues: &[Issue]) -> OwaspDashboard {
    let total = issues.len();
    let mut buckets: [(OwaspCategory, usize); 11] = [
        (OwaspCategory::A01BrokenAccessControl, 0),
        (OwaspCategory::A02CryptographicFailures, 0),
        (OwaspCategory::A03Injection, 0),
        (OwaspCategory::A04InsecureDesign, 0),
        (OwaspCategory::A05SecurityMisconfiguration, 0),
        (OwaspCategory::A06VulnerableComponents, 0),
        (OwaspCategory::A07AuthenticationFailures, 0),
        (OwaspCategory::A08DataIntegrityFailures, 0),
        (OwaspCategory::A09LoggingMonitoring, 0),
        (OwaspCategory::A10Ssrf, 0),
        (OwaspCategory::Other, 0),
    ];
    for issue in issues {
        let cat = category_for_rule(&issue.rule_id);
        if let Some(b) = buckets.iter_mut().find(|(c, _)| *c == cat) {
            b.1 += 1;
        }
    }
    let categories = buckets
        .iter()
        .map(|(cat, count)| {
            let percent = if total > 0 {
                (*count as f64) / (total as f64) * 100.0
            } else {
                0.0
            };
            let baseline = industry_percent(*cat);
            CategoryStat {
                code: cat.code().to_string(),
                title: cat.title().to_string(),
                count: *count,
                percent,
                industry_baseline: baseline,
                delta_pp: percent - baseline,
            }
        })
        .collect();
    OwaspDashboard { total, categories }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scanner::{IssueConfidence, IssueSeverity};

    fn issue(rule: &str) -> Issue {
        Issue {
            id: format!("id-{rule}"),
            flow_id: String::new(),
            rule_id: rule.to_string(),
            name: rule.to_string(),
            severity: IssueSeverity::Info,
            confidence: IssueConfidence::Firm,
            description: String::new(),
            evidence: None,
            remediation: None,
            host: "example".to_string(),
            path: "/".to_string(),
        }
    }

    #[test]
    fn empty_dashboard_is_well_formed() {
        let d = build(&[]);
        assert_eq!(d.total, 0);
        assert_eq!(d.categories.len(), 11);
        for c in &d.categories {
            assert_eq!(c.count, 0);
            assert!((c.percent - 0.0).abs() < 1e-9);
            // delta_pp = 0 − baseline = negative baseline.
            assert!((c.delta_pp + c.industry_baseline).abs() < 1e-9);
        }
    }

    #[test]
    fn percent_sums_to_100_when_no_other() {
        let issues = vec![
            issue("sql-injection"),
            issue("xss-reflected"),
            issue("ssrf"),
            issue("jwt-alg-none"),
        ];
        let d = build(&issues);
        let sum: f64 = d.categories.iter().map(|c| c.percent).sum();
        assert!((sum - 100.0).abs() < 1e-9, "sum was {}", sum);
    }

    #[test]
    fn delta_shows_overrepresentation_in_injection() {
        // 3 of 4 issues are injection → 75% vs ~13% baseline → +62pp.
        let issues = vec![
            issue("sql-injection"),
            issue("xss-reflected"),
            issue("xss-stored"),
            issue("server-banner"),
        ];
        let d = build(&issues);
        let inj = d.categories.iter().find(|c| c.code == "A03").unwrap();
        assert_eq!(inj.count, 3);
        assert!((inj.percent - 75.0).abs() < 1e-9);
        assert!(inj.delta_pp > 50.0);
    }

    #[test]
    fn unknown_rule_lands_in_other_bucket() {
        let d = build(&[issue("totally-made-up")]);
        let other = d.categories.iter().find(|c| c.code == "OTH").unwrap();
        assert_eq!(other.count, 1);
        assert!((other.percent - 100.0).abs() < 1e-9);
    }
}
