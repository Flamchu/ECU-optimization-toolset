//! `Rule`, `Finding`, `Severity` — base types for the AMF rule pack.
//!
//! Per spec §4.3: every rule carries an `id`, a `severity`, a one-line
//! rationale, and an optional reference to a row in the default-deltas
//! table. Predicates evaluate per WOT pull and return zero or more
//! `Finding`s.

use crate::util::Pull;

/// Severity ordering: `info < warn < critical`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    /// Informational; no action required.
    Info,
    /// Something is off; investigate before tuning further.
    Warn,
    /// A longevity envelope is breached; do not raise.
    Critical,
}

impl Severity {
    /// Lower-case display string used in reports.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Info => "info",
            Self::Warn => "warn",
            Self::Critical => "critical",
        }
    }
}

/// One rule firing on one pull (or a skipped placeholder).
#[derive(Debug, Clone)]
pub struct Finding {
    /// Rule id (e.g. `"R02"`).
    pub rule_id: &'static str,
    /// Severity at which this finding fired (rules can downgrade).
    pub severity: Severity,
    /// Pull id this finding belongs to.
    pub pull_id: u32,
    /// Pull start time (seconds).
    pub t_start: f64,
    /// Pull end time (seconds).
    pub t_end: f64,
    /// Most-extreme observed value that triggered the rule. Units are
    /// rule-dependent.
    pub observed_extreme: f64,
    /// Threshold the observation breached.
    pub threshold: f64,
    /// One-line physical/longevity reason for the rule.
    pub rationale: String,
    /// Key into the default-deltas table, if applicable.
    pub recommended_action_ref: Option<String>,
    /// True when the rule could not evaluate (missing channels). The
    /// observed/threshold fields are unset/0 in that case.
    pub skipped: bool,
}

impl Finding {
    /// Compact one-line summary suitable for terminal output.
    pub fn short(&self) -> String {
        format!(
            "[{}] {} pull#{}: observed {} vs threshold {}",
            self.severity.as_str().to_ascii_uppercase(),
            self.rule_id,
            self.pull_id,
            short_num(self.observed_extreme),
            short_num(self.threshold),
        )
    }
}

fn short_num(x: f64) -> String {
    if x.fract() == 0.0 { format!("{x:.0}") } else { format!("{x:.4}") }
}

/// Declarative rule wrapper. The predicate is a free function in
/// [`crate::rules::pack`].
#[derive(Debug, Clone, Copy)]
pub struct Rule {
    /// Rule id.
    pub id: &'static str,
    /// Default severity.
    pub severity: Severity,
    /// One-line rationale.
    pub rationale_one_liner: &'static str,
    /// Default-deltas key, if any.
    pub recommended_delta_ref: Option<&'static str>,
    /// Channels the rule needs to evaluate.
    pub requires_channels: &'static [&'static str],
    /// VCDS groups the rule needs to evaluate.
    pub requires_groups: &'static [&'static str],
}

/// Helper: emit a SKIPPED `Finding` when a rule cannot evaluate.
pub fn make_skipped(rule: &Rule, pull: &Pull, reason: &str) -> Finding {
    Finding {
        rule_id: rule.id,
        severity: Severity::Info,
        pull_id: pull.pull_id,
        t_start: pull.t_start,
        t_end: pull.t_end,
        observed_extreme: 0.0,
        threshold: 0.0,
        rationale: format!("SKIPPED — {reason}"),
        recommended_action_ref: rule.recommended_delta_ref.map(str::to_string),
        skipped: true,
    }
}
