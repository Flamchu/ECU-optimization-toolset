//! Run the v4 rule pack across every detected pull (per-pull rules) and
//! exactly once over the whole log (global rules — R16, R19, R21).

use std::collections::BTreeSet;

use crate::ingest::VcdsLog;
use crate::rules::base::{
    make_skipped, synthetic_global_pull, Finding, RuleScope, Severity,
};
use crate::rules::pack::{dispatch, ALL_RULES};
use crate::util::pulls::detect_pulls;
use crate::util::{Pull, ResampledLog};

/// Result of analysing one log: parser metadata, the resampled
/// dataframe, the detected pulls, and all collected findings.
pub struct AnalysisResult {
    /// The original parsed log (kept for metadata + DTC sidecar).
    pub log: VcdsLog,
    /// Uniformly-sampled log used by the rules.
    pub df: ResampledLog,
    /// Detected WOT pulls.
    pub pulls: Vec<Pull>,
    /// Findings produced across all rules and all pulls.
    pub findings: Vec<Finding>,
    /// Rule ids that were skipped wholesale because a required group is
    /// not present in the log.
    pub skipped_rules: Vec<String>,
}

impl AnalysisResult {
    /// Findings at the `critical` level.
    pub fn critical(&self) -> Vec<&Finding> {
        self.findings.iter().filter(|f| f.severity == Severity::Critical && !f.skipped).collect()
    }

    /// Findings at the `warn` level.
    pub fn warn(&self) -> Vec<&Finding> {
        self.findings.iter().filter(|f| f.severity == Severity::Warn && !f.skipped).collect()
    }

    /// Findings at the `info` level (excluding skipped placeholders).
    pub fn info(&self) -> Vec<&Finding> {
        self.findings.iter().filter(|f| f.severity == Severity::Info && !f.skipped).collect()
    }
}

/// Run every rule against every pull (per-pull) and once over the whole
/// log (global). Returns `(findings, all_skipped_rule_ids)`.
pub fn run_rules(
    df: &ResampledLog,
    log_meta: &VcdsLog,
    pulls: &[Pull],
) -> (Vec<Finding>, Vec<String>) {
    let mut findings: Vec<Finding> = Vec::new();
    let mut skipped: BTreeSet<String> = BTreeSet::new();

    let global_pull = synthetic_global_pull(
        df.len(),
        df.time.first().copied().unwrap_or(0.0),
        df.time.last().copied().unwrap_or(0.0),
    );

    for rule in ALL_RULES {
        let missing: Vec<String> = rule.requires_groups.iter()
            .filter(|g| !log_meta.groups.contains(**g))
            .map(|s| s.to_string())
            .collect();
        if !missing.is_empty() {
            // Surface a skipped placeholder per pull (or once for global rules
            // when no pulls exist).
            let placeholder_pulls: Vec<&Pull> = match rule.scope {
                RuleScope::PerPull => pulls.iter().collect(),
                RuleScope::Global => vec![&global_pull],
            };
            if placeholder_pulls.is_empty() {
                let reason = format!("required VCDS group(s) {missing:?} not present");
                findings.push(make_skipped(rule, &global_pull, &reason));
            } else {
                for pull in placeholder_pulls {
                    let reason = format!("required VCDS group(s) {missing:?} not present");
                    findings.push(make_skipped(rule, pull, &reason));
                }
            }
            skipped.insert(rule.id.to_string());
            continue;
        }

        match rule.scope {
            RuleScope::PerPull => {
                for pull in pulls {
                    findings.extend(dispatch(rule, df, &log_meta.dtcs, pull, log_meta.low_rate()));
                }
            }
            RuleScope::Global => {
                findings.extend(dispatch(rule, df, &log_meta.dtcs, &global_pull, log_meta.low_rate()));
            }
        }
    }

    (findings, skipped.into_iter().collect())
}

/// One-shot helper: detect pulls, run rules, return [`AnalysisResult`].
pub fn analyse(df: ResampledLog, log_meta: VcdsLog) -> AnalysisResult {
    let pulls = detect_pulls(&df);
    let (findings, skipped_rules) = run_rules(&df, &log_meta, &pulls);
    AnalysisResult { log: log_meta, df, pulls, findings, skipped_rules }
}
