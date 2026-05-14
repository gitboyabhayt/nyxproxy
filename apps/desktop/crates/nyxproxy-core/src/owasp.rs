//! Maps scanner rule identifiers to OWASP Top 10 (2021) categories.
//!
//! The mapping is deterministic and lives in code (not a config file) so the
//! UI can rely on it without an extra round-trip. Add new rules to
//! [`category_for_rule`] when the scanner grows.
//!
//! Source of category definitions: <https://owasp.org/Top10/>

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum OwaspCategory {
    /// A01:2021 — Broken Access Control
    A01BrokenAccessControl,
    /// A02:2021 — Cryptographic Failures
    A02CryptographicFailures,
    /// A03:2021 — Injection
    A03Injection,
    /// A04:2021 — Insecure Design
    A04InsecureDesign,
    /// A05:2021 — Security Misconfiguration
    A05SecurityMisconfiguration,
    /// A06:2021 — Vulnerable and Outdated Components
    A06VulnerableComponents,
    /// A07:2021 — Identification and Authentication Failures
    A07AuthenticationFailures,
    /// A08:2021 — Software and Data Integrity Failures
    A08DataIntegrityFailures,
    /// A09:2021 — Security Logging and Monitoring Failures
    A09LoggingMonitoring,
    /// A10:2021 — Server-Side Request Forgery
    A10Ssrf,
    /// Rule does not cleanly fit any Top 10 entry.
    Other,
}

impl OwaspCategory {
    /// Stable short code suitable for badges (`A05`, `A02`, `OTH`).
    pub fn code(self) -> &'static str {
        match self {
            Self::A01BrokenAccessControl => "A01",
            Self::A02CryptographicFailures => "A02",
            Self::A03Injection => "A03",
            Self::A04InsecureDesign => "A04",
            Self::A05SecurityMisconfiguration => "A05",
            Self::A06VulnerableComponents => "A06",
            Self::A07AuthenticationFailures => "A07",
            Self::A08DataIntegrityFailures => "A08",
            Self::A09LoggingMonitoring => "A09",
            Self::A10Ssrf => "A10",
            Self::Other => "OTH",
        }
    }

    /// Human-readable title for tooltips and reports.
    pub fn title(self) -> &'static str {
        match self {
            Self::A01BrokenAccessControl => "Broken Access Control",
            Self::A02CryptographicFailures => "Cryptographic Failures",
            Self::A03Injection => "Injection",
            Self::A04InsecureDesign => "Insecure Design",
            Self::A05SecurityMisconfiguration => "Security Misconfiguration",
            Self::A06VulnerableComponents => "Vulnerable and Outdated Components",
            Self::A07AuthenticationFailures => "Identification and Authentication Failures",
            Self::A08DataIntegrityFailures => "Software and Data Integrity Failures",
            Self::A09LoggingMonitoring => "Security Logging and Monitoring Failures",
            Self::A10Ssrf => "Server-Side Request Forgery",
            Self::Other => "Other",
        }
    }
}

/// Look up the OWASP category for a scanner rule identifier.
///
/// Returns [`OwaspCategory::Other`] for unknown rules so the UI can still
/// render a generic badge.
pub fn category_for_rule(rule_id: &str) -> OwaspCategory {
    match rule_id {
        "missing-security-headers"
        | "info-disclosure"
        | "directory-listing"
        | "dangerous-methods"
        | "cors-wildcard-creds"
        | "server-banner" => OwaspCategory::A05SecurityMisconfiguration,

        "cookie-flags" | "mixed-content" | "sensitive-in-url" | "basic-auth-http" => {
            OwaspCategory::A02CryptographicFailures
        }

        "jwt-alg-none" => OwaspCategory::A07AuthenticationFailures,
        "open-redirect-hint" => OwaspCategory::A01BrokenAccessControl,
        "sql-injection" | "xss-reflected" | "xss-stored" | "ssti" | "command-injection" => {
            OwaspCategory::A03Injection
        }
        "ssrf" | "ssrf-collaborator" => OwaspCategory::A10Ssrf,
        "outdated-component" | "known-cve" => OwaspCategory::A06VulnerableComponents,
        "weak-jwt-signature" | "missing-csrf" => OwaspCategory::A07AuthenticationFailures,
        "integrity-failure" | "subresource-integrity" => OwaspCategory::A08DataIntegrityFailures,
        "no-audit-log" => OwaspCategory::A09LoggingMonitoring,
        _ => OwaspCategory::Other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_rules_map_correctly() {
        assert_eq!(
            category_for_rule("missing-security-headers"),
            OwaspCategory::A05SecurityMisconfiguration
        );
        assert_eq!(
            category_for_rule("jwt-alg-none"),
            OwaspCategory::A07AuthenticationFailures
        );
        assert_eq!(category_for_rule("ssrf"), OwaspCategory::A10Ssrf);
        assert_eq!(
            category_for_rule("sql-injection"),
            OwaspCategory::A03Injection
        );
    }

    #[test]
    fn unknown_rule_falls_back_to_other() {
        assert_eq!(category_for_rule("totally-made-up"), OwaspCategory::Other);
    }

    #[test]
    fn codes_are_three_chars() {
        for cat in [
            OwaspCategory::A01BrokenAccessControl,
            OwaspCategory::A05SecurityMisconfiguration,
            OwaspCategory::A10Ssrf,
            OwaspCategory::Other,
        ] {
            assert_eq!(cat.code().len(), 3);
        }
    }
}
