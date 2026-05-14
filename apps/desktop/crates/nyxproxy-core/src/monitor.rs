//! Continuous monitoring (Feature AA).
//!
//! Stores recurring scan *schedules* (target URL + cadence + scope) and
//! a *baseline* of issues per schedule so subsequent runs can diff
//! against the previous run and surface only **new** findings.
//!
//! The runtime that actually fires scans on a clock lives in the Tauri
//! layer (we do not want to depend on `tokio::time` from a pure logic
//! crate that is also used in WASM-style smoke tests). The state below
//! is `Send + Sync` and intended to be wrapped in `Arc<Mutex<...>>`.

use crate::scanner::{Issue, IssueConfidence, IssueSeverity};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Cadence {
    Hourly,
    Daily,
    Weekly,
}

impl Cadence {
    pub fn interval(self) -> Duration {
        match self {
            Cadence::Hourly => Duration::hours(1),
            Cadence::Daily => Duration::days(1),
            Cadence::Weekly => Duration::days(7),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MonitorSchedule {
    pub id: Uuid,
    pub name: String,
    pub target_url: String,
    pub scope_hosts: Vec<String>,
    pub cadence: Cadence,
    pub created_at: DateTime<Utc>,
    pub last_run_at: Option<DateTime<Utc>>,
    pub next_run_at: DateTime<Utc>,
    pub enabled: bool,
    /// Fingerprints of issues seen in the most recent successful run.
    pub baseline_fingerprints: Vec<String>,
}

impl MonitorSchedule {
    pub fn new(
        name: impl Into<String>,
        target_url: impl Into<String>,
        scope_hosts: Vec<String>,
        cadence: Cadence,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            target_url: target_url.into(),
            scope_hosts,
            cadence,
            created_at: now,
            last_run_at: None,
            next_run_at: now,
            enabled: true,
            baseline_fingerprints: Vec::new(),
        }
    }

    pub fn is_due(&self, at: DateTime<Utc>) -> bool {
        self.enabled && at >= self.next_run_at
    }
}

/// One persisted run report.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MonitorRunRecord {
    pub schedule_id: Uuid,
    pub started_at: DateTime<Utc>,
    pub finished_at: DateTime<Utc>,
    pub new_issues: Vec<Issue>,
    pub resolved_issues: Vec<Issue>,
    pub still_present: usize,
    pub error: Option<String>,
}

/// In-memory schedule store. Intended to be wrapped in
/// `Arc<Mutex<...>>` by the Tauri layer.
#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct MonitorState {
    pub schedules: HashMap<Uuid, MonitorSchedule>,
    pub runs: Vec<MonitorRunRecord>,
}

impl MonitorState {
    pub fn upsert(&mut self, schedule: MonitorSchedule) {
        self.schedules.insert(schedule.id, schedule);
    }

    pub fn remove(&mut self, id: Uuid) -> Option<MonitorSchedule> {
        self.schedules.remove(&id)
    }

    pub fn list(&self) -> Vec<MonitorSchedule> {
        let mut v: Vec<MonitorSchedule> = self.schedules.values().cloned().collect();
        v.sort_by_key(|s| s.created_at);
        v
    }

    pub fn due(&self, at: DateTime<Utc>) -> Vec<MonitorSchedule> {
        self.schedules
            .values()
            .filter(|s| s.is_due(at))
            .cloned()
            .collect()
    }

    /// Record a completed run. Computes new/resolved issues against the
    /// schedule's baseline, then updates the baseline + next_run_at.
    pub fn complete_run(
        &mut self,
        schedule_id: Uuid,
        started_at: DateTime<Utc>,
        finished_at: DateTime<Utc>,
        issues: &[Issue],
        error: Option<String>,
    ) -> Option<MonitorRunRecord> {
        let schedule = self.schedules.get_mut(&schedule_id)?;

        let current: Vec<String> = issues.iter().map(fingerprint).collect();
        let baseline: std::collections::HashSet<&String> =
            schedule.baseline_fingerprints.iter().collect();
        let current_set: std::collections::HashSet<&String> = current.iter().collect();

        let new_issues: Vec<Issue> = issues
            .iter()
            .filter(|i| !baseline.contains(&fingerprint(i)))
            .cloned()
            .collect();

        let resolved_issues: Vec<Issue> = schedule
            .baseline_fingerprints
            .iter()
            .filter(|fp| !current_set.contains(*fp))
            .map(|fp| placeholder_issue(fp))
            .collect();

        let still_present = current
            .iter()
            .filter(|fp| baseline.contains(*fp))
            .count();

        // Update the schedule.
        if error.is_none() {
            schedule.baseline_fingerprints = current;
        }
        schedule.last_run_at = Some(finished_at);
        schedule.next_run_at = finished_at + schedule.cadence.interval();

        let record = MonitorRunRecord {
            schedule_id,
            started_at,
            finished_at,
            new_issues,
            resolved_issues,
            still_present,
            error,
        };
        self.runs.push(record.clone());
        // Keep only last 200 runs per state to bound memory.
        if self.runs.len() > 200 {
            let excess = self.runs.len() - 200;
            self.runs.drain(0..excess);
        }
        Some(record)
    }
}

/// Stable fingerprint for diffing issues across runs.
pub fn fingerprint(issue: &Issue) -> String {
    let sev = match issue.severity {
        IssueSeverity::Info => "info",
        IssueSeverity::Low => "low",
        IssueSeverity::Medium => "medium",
        IssueSeverity::High => "high",
        IssueSeverity::Critical => "critical",
    };
    format!(
        "{}|{}|{}|{}",
        issue.rule_id, sev, issue.host, issue.path
    )
}

fn placeholder_issue(fp: &str) -> Issue {
    let mut parts = fp.splitn(4, '|');
    let rule_id = parts.next().unwrap_or("unknown").to_string();
    let sev_s = parts.next().unwrap_or("info");
    let severity = match sev_s {
        "critical" => IssueSeverity::Critical,
        "high" => IssueSeverity::High,
        "medium" => IssueSeverity::Medium,
        "low" => IssueSeverity::Low,
        _ => IssueSeverity::Info,
    };
    let host = parts.next().unwrap_or("").to_string();
    let path = parts.next().unwrap_or("").to_string();
    Issue {
        id: format!("resolved-{rule_id}-{host}-{path}"),
        flow_id: String::new(),
        rule_id,
        name: "Resolved finding".to_string(),
        severity,
        confidence: IssueConfidence::Firm,
        description: "Was present in the previous run; no longer detected.".to_string(),
        evidence: None,
        remediation: None,
        host,
        path,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scanner::{Issue, IssueConfidence, IssueSeverity};

    fn mk_issue(rule: &str, host: &str, path: &str, sev: IssueSeverity) -> Issue {
        Issue {
            id: format!("{rule}-{host}{path}"),
            flow_id: String::new(),
            rule_id: rule.to_string(),
            name: rule.to_string(),
            severity: sev,
            confidence: IssueConfidence::Firm,
            description: String::new(),
            evidence: None,
            remediation: None,
            host: host.to_string(),
            path: path.to_string(),
        }
    }

    #[test]
    fn cadence_intervals_are_correct() {
        assert_eq!(Cadence::Hourly.interval(), Duration::hours(1));
        assert_eq!(Cadence::Daily.interval(), Duration::days(1));
        assert_eq!(Cadence::Weekly.interval(), Duration::days(7));
    }

    #[test]
    fn schedule_is_due_when_next_run_passed() {
        let mut sched = MonitorSchedule::new(
            "demo",
            "https://api.example/",
            vec!["api.example".into()],
            Cadence::Daily,
        );
        let now = Utc::now();
        sched.next_run_at = now - Duration::seconds(1);
        assert!(sched.is_due(now));

        sched.enabled = false;
        assert!(!sched.is_due(now));

        sched.enabled = true;
        sched.next_run_at = now + Duration::hours(1);
        assert!(!sched.is_due(now));
    }

    #[test]
    fn first_run_marks_everything_as_new() {
        let mut state = MonitorState::default();
        let sched = MonitorSchedule::new(
            "x",
            "https://a/",
            vec!["a".into()],
            Cadence::Hourly,
        );
        let sid = sched.id;
        state.upsert(sched);

        let issues = vec![
            mk_issue("xss", "a", "/q", IssueSeverity::High),
            mk_issue("info", "a", "/", IssueSeverity::Low),
        ];
        let now = Utc::now();
        let rec = state
            .complete_run(sid, now, now, &issues, None)
            .expect("schedule exists");
        assert_eq!(rec.new_issues.len(), 2);
        assert_eq!(rec.resolved_issues.len(), 0);
        assert_eq!(rec.still_present, 0);

        let s2 = state.schedules.get(&sid).unwrap();
        assert_eq!(s2.baseline_fingerprints.len(), 2);
        assert!(s2.next_run_at > now);
    }

    #[test]
    fn second_run_diffs_against_baseline() {
        let mut state = MonitorState::default();
        let sched = MonitorSchedule::new(
            "x",
            "https://a/",
            vec!["a".into()],
            Cadence::Hourly,
        );
        let sid = sched.id;
        state.upsert(sched);

        let initial = vec![
            mk_issue("xss", "a", "/q", IssueSeverity::High),
            mk_issue("info", "a", "/", IssueSeverity::Low),
        ];
        let now = Utc::now();
        state.complete_run(sid, now, now, &initial, None);

        let next = vec![
            mk_issue("xss", "a", "/q", IssueSeverity::High),
            mk_issue("sqli", "a", "/login", IssueSeverity::Critical),
        ];
        let later = now + Duration::hours(1);
        let rec = state
            .complete_run(sid, later, later, &next, None)
            .unwrap();

        // 1 new finding (sqli), 1 resolved (info), 1 still present (xss).
        assert_eq!(rec.new_issues.len(), 1);
        assert_eq!(rec.new_issues[0].rule_id, "sqli");
        assert_eq!(rec.resolved_issues.len(), 1);
        assert_eq!(rec.resolved_issues[0].rule_id, "info");
        assert_eq!(rec.still_present, 1);
    }

    #[test]
    fn errored_run_does_not_update_baseline() {
        let mut state = MonitorState::default();
        let sched = MonitorSchedule::new(
            "x",
            "https://a/",
            vec!["a".into()],
            Cadence::Hourly,
        );
        let sid = sched.id;
        state.upsert(sched);

        let initial = vec![mk_issue("xss", "a", "/q", IssueSeverity::High)];
        let t0 = Utc::now();
        state.complete_run(sid, t0, t0, &initial, None);
        let baseline_before = state
            .schedules
            .get(&sid)
            .unwrap()
            .baseline_fingerprints
            .clone();

        // Errored run: scan didn't finish, so baseline must NOT change.
        let t1 = t0 + Duration::hours(1);
        state.complete_run(sid, t1, t1, &[], Some("timeout".into()));
        let baseline_after = state
            .schedules
            .get(&sid)
            .unwrap()
            .baseline_fingerprints
            .clone();
        assert_eq!(baseline_before, baseline_after);
    }

    #[test]
    fn fingerprint_is_stable() {
        let a = mk_issue("xss", "a", "/q", IssueSeverity::High);
        let b = mk_issue("xss", "a", "/q", IssueSeverity::High);
        assert_eq!(fingerprint(&a), fingerprint(&b));

        let c = mk_issue("xss", "a", "/Q", IssueSeverity::High);
        assert_ne!(fingerprint(&a), fingerprint(&c));
    }

    #[test]
    fn runs_are_bounded_at_two_hundred() {
        let mut state = MonitorState::default();
        let sched = MonitorSchedule::new("x", "u", vec![], Cadence::Hourly);
        let sid = sched.id;
        state.upsert(sched);
        let now = Utc::now();
        for _ in 0..250 {
            state.complete_run(sid, now, now, &[], None);
        }
        assert_eq!(state.runs.len(), 200);
    }
}
