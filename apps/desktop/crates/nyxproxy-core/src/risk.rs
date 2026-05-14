//! Deterministic risk-score engine for scanner issues.
//!
//! Each issue is reduced to a numeric score in `[0, 100]` so the UI can sort
//! and aggregate without re-implementing the priority logic per page. The
//! formula is intentionally simple — severity contributes the bulk of the
//! score, confidence acts as a multiplier, and OWASP category adds a small
//! bias toward access-control / injection findings, which usually have more
//! direct business impact than misconfigurations.
//!
//! The function is pure so unit tests cover the full output surface.

use crate::owasp::{category_for_rule, OwaspCategory};
use crate::scanner::{Issue, IssueConfidence, IssueSeverity};

/// Risk score for a single issue, clamped to `[0, 100]`.
pub fn score_issue(issue: &Issue) -> u8 {
    let sev_base: f32 = match issue.severity {
        IssueSeverity::Critical => 95.0,
        IssueSeverity::High => 75.0,
        IssueSeverity::Medium => 50.0,
        IssueSeverity::Low => 25.0,
        IssueSeverity::Info => 5.0,
    };
    let conf_mult: f32 = match issue.confidence {
        IssueConfidence::Certain => 1.0,
        IssueConfidence::Firm => 0.85,
        IssueConfidence::Tentative => 0.6,
    };
    let category_bias: f32 = match category_for_rule(&issue.rule_id) {
        OwaspCategory::A01BrokenAccessControl | OwaspCategory::A03Injection => 5.0,
        OwaspCategory::A10Ssrf => 4.0,
        OwaspCategory::A02CryptographicFailures | OwaspCategory::A07AuthenticationFailures => 3.0,
        OwaspCategory::A05SecurityMisconfiguration => 1.0,
        _ => 0.0,
    };
    let raw = (sev_base * conf_mult) + category_bias;
    raw.clamp(0.0, 100.0).round() as u8
}

/// Aggregate score for a batch of issues — the maximum single-issue score
/// rather than a sum, so adding low-noise findings doesn't inflate the total.
pub fn score_aggregate(issues: &[Issue]) -> u8 {
    issues.iter().map(score_issue).max().unwrap_or(0)
}

/// Sort an issue list in-place by descending risk score, breaking ties by
/// `rule_id` for stable ordering.
pub fn sort_by_risk_desc(issues: &mut [Issue]) {
    issues.sort_by(|a, b| {
        let sa = score_issue(a);
        let sb = score_issue(b);
        sb.cmp(&sa).then_with(|| a.rule_id.cmp(&b.rule_id))
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scanner::{Issue, IssueConfidence, IssueSeverity};

    fn make_issue(rule: &str, sev: IssueSeverity, conf: IssueConfidence) -> Issue {
        Issue {
            id: "id".into(),
            flow_id: "flow".into(),
            rule_id: rule.into(),
            name: rule.into(),
            severity: sev,
            confidence: conf,
            description: "".into(),
            evidence: None,
            remediation: None,
            host: "h".into(),
            path: "/p".into(),
        }
    }

    #[test]
    fn critical_certain_injection_scores_at_ceiling() {
        let i = make_issue("sql-injection", IssueSeverity::Critical, IssueConfidence::Certain);
        assert_eq!(score_issue(&i), 100);
    }

    #[test]
    fn info_tentative_other_scores_near_floor() {
        let i = make_issue("unknown", IssueSeverity::Info, IssueConfidence::Tentative);
        // 5 * 0.6 + 0 = 3
        assert_eq!(score_issue(&i), 3);
    }

    #[test]
    fn high_firm_ssrf_scores_below_certain_injection() {
        let ssrf = make_issue("ssrf", IssueSeverity::High, IssueConfidence::Firm);
        let inj = make_issue("sql-injection", IssueSeverity::Critical, IssueConfidence::Certain);
        assert!(score_issue(&ssrf) < score_issue(&inj));
        assert!(score_issue(&ssrf) >= 60);
    }

    #[test]
    fn aggregate_is_max_not_sum() {
        let issues = vec![
            make_issue("missing-security-headers", IssueSeverity::Low, IssueConfidence::Firm),
            make_issue("sql-injection", IssueSeverity::High, IssueConfidence::Firm),
            make_issue("server-banner", IssueSeverity::Info, IssueConfidence::Tentative),
        ];
        let agg = score_aggregate(&issues);
        let max_single = issues.iter().map(score_issue).max().unwrap();
        assert_eq!(agg, max_single);
    }

    #[test]
    fn sort_orders_high_risk_first() {
        let mut issues = vec![
            make_issue("server-banner", IssueSeverity::Info, IssueConfidence::Tentative),
            make_issue("sql-injection", IssueSeverity::Critical, IssueConfidence::Certain),
            make_issue("cookie-flags", IssueSeverity::Low, IssueConfidence::Firm),
        ];
        sort_by_risk_desc(&mut issues);
        assert_eq!(issues[0].rule_id, "sql-injection");
        assert_eq!(issues[2].rule_id, "server-banner");
    }

    #[test]
    fn empty_aggregate_is_zero() {
        assert_eq!(score_aggregate(&[]), 0);
    }
}
