//! Run the rule pack across every detected pull and aggregate findings.

use std::collections::BTreeSet;

use crate::ingest::VcdsLog;
use crate::rules::base::{make_skipped, Finding, Severity};
use crate::rules::pack::{dispatch, ALL_RULES};
use crate::util::pulls::detect_pulls;
use crate::util::{Pull, ResampledLog};

/// Result of analysing one log: parser metadata, the resampled
/// dataframe, the detected pulls, and all collected findings.
pub struct AnalysisResult {
    /// The original parsed log (kept for metadata only).
    pub log: VcdsLog,
    /// Uniformly-sampled log used by the rules.
    pub df: ResampledLog,
    /// Detected WOT pulls.
    pub pulls: Vec<Pull>,
    /// Findings produced across all rules and all pulls.
    pub findings: Vec<Finding>,
    /// Rule ids that were skipped wholesale because a required group
    /// is not present in the log.
    pub skipped_rules: Vec<String>,
}

impl AnalysisResult {
    /// Findings at the `critical` level.
    pub fn critical(&self) -> Vec<&Finding> {
        self.findings.iter().filter(|f| f.severity == Severity::Critical).collect()
    }

    /// Findings at the `warn` level.
    pub fn warn(&self) -> Vec<&Finding> {
        self.findings.iter().filter(|f| f.severity == Severity::Warn).collect()
    }

    /// Findings at the `info` level.
    pub fn info(&self) -> Vec<&Finding> {
        self.findings.iter().filter(|f| f.severity == Severity::Info).collect()
    }
}

/// Rules that downgrade severity on `LOW_RATE` logs.
const LOW_RATE_AWARE: &[&str] = &["R09", "R10"];

/// Run every rule against every pull. Returns
/// `(findings, all_skipped_rule_ids)`.
pub fn run_rules(
    df: &ResampledLog,
    log_meta: &VcdsLog,
    pulls: &[Pull],
) -> (Vec<Finding>, Vec<String>) {
    let mut findings: Vec<Finding> = Vec::new();
    let mut skipped: BTreeSet<String> = BTreeSet::new();

    if pulls.is_empty() {
        return (findings, skipped.into_iter().collect());
    }

    for rule in ALL_RULES {
        let missing: Vec<String> = rule.requires_groups.iter()
            .filter(|g| !log_meta.groups.contains(**g))
            .map(|s| s.to_string())
            .collect();
        if !missing.is_empty() {
            for pull in pulls {
                let reason = format!("required VCDS group(s) {missing:?} not present");
                findings.push(make_skipped(rule, pull, &reason));
            }
            skipped.insert(rule.id.to_string());
            continue;
        }

        let _aware = LOW_RATE_AWARE.contains(&rule.id);
        for pull in pulls {
            findings.extend(dispatch(rule, df, pull, log_meta.low_rate()));
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
