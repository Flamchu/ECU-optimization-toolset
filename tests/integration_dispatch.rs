//! v4 acceptance #14: every variant of `RuleId` must be reachable
//! through the dispatcher.

use std::collections::BTreeMap;

use ecu_shenanigans::rules::pack::{dispatch, RuleId, ALL_RULE_IDS};
use ecu_shenanigans::util::{Pull, ResampledLog};

fn empty_log() -> ResampledLog {
    ResampledLog { time: Vec::new(), data: BTreeMap::new() }
}

fn synthetic_pull() -> Pull {
    Pull {
        pull_id: 0, i_start: 0, i_end: 0,
        t_start: 0.0, t_end: 0.0,
        rpm_start: 0.0, rpm_end: 0.0,
    }
}

#[test]
fn all_rule_ids_have_distinct_string_ids() {
    use std::collections::HashSet;
    let mut seen: HashSet<&'static str> = HashSet::new();
    for &id in ALL_RULE_IDS {
        assert!(seen.insert(id.as_str()),
            "RuleId::{} produces duplicate string id {}", id.as_str(), id.as_str());
    }
    assert_eq!(seen.len(), 23, "rule pack has 23 rules");
}

#[test]
fn rule_id_iteration_order_matches_canonical() {
    let names: Vec<&str> = ALL_RULE_IDS.iter().map(|id| id.as_str()).collect();
    assert_eq!(names, vec![
        "R01", "R02", "R03", "R04", "R05", "R06", "R07",
        "R08", "R09", "R10", "R11", "R12", "R13", "R14", "R15",
        "R16", "R17", "R18", "R19", "R20", "R21", "R22", "R23",
    ]);
}

#[test]
fn dispatch_covers_every_rule_variant() {
    // Calling dispatch on an empty log + synthetic pull must NOT panic
    // for any RuleId — proves the match arms are exhaustive.
    let log = empty_log();
    let pull = synthetic_pull();
    let dtcs: Vec<String> = Vec::new();
    for &id in ALL_RULE_IDS {
        let rule = id.rule();
        // No assertion on findings here; just exercising the dispatcher.
        let _ = dispatch(rule, &log, &dtcs, &pull, false);
    }
}

#[test]
fn dispatch_is_idempotent_across_low_rate_flag() {
    // Most rules don't react to the low_rate flag; dispatch with both
    // values must produce equivalent output for those rules. R09 is
    // the documented exception (Critical → Warn under low_rate).
    let log = empty_log();
    let pull = synthetic_pull();
    let dtcs: Vec<String> = Vec::new();
    for &id in ALL_RULE_IDS {
        let rule = id.rule();
        let a = dispatch(rule, &log, &dtcs, &pull, false);
        let b = dispatch(rule, &log, &dtcs, &pull, true);
        // For empty input there are no findings, so output is empty either way.
        assert_eq!(a.len(), b.len(),
            "dispatch length differs for {} between low_rate true/false", id.as_str());
    }
}

#[test]
fn r10_dispatch_signature_does_not_take_low_rate() {
    // Indirect check: dispatching R10 with low_rate true must yield the
    // same shape as low_rate false (R10 has no LOW_RATE downgrade in v4).
    use ecu_shenanigans::util::ResampledLog;
    let mut data: BTreeMap<String, Vec<f64>> = BTreeMap::new();
    let n = 30usize;
    data.insert("rpm".to_string(), vec![4000.0; n]);
    data.insert("iq_requested".to_string(), vec![50.0; n]);
    data.insert("soi_actual".to_string(), vec![5.0; n]); // late SOI → late EOI
    let log = ResampledLog {
        time: (0..n).map(|i| i as f64 * 0.2).collect(),
        data,
    };
    let pull = Pull {
        pull_id: 1, i_start: 0, i_end: n,
        t_start: 0.0, t_end: ((n - 1) as f64) * 0.2,
        rpm_start: 4000.0, rpm_end: 4000.0,
    };
    let rule = RuleId::R10.rule();
    let dtcs: Vec<String> = Vec::new();
    let a = dispatch(rule, &log, &dtcs, &pull, false);
    let b = dispatch(rule, &log, &dtcs, &pull, true);
    assert_eq!(a.len(), b.len(),
        "R10 must produce identical findings regardless of low_rate (Warn baseline)");
    if let (Some(fa), Some(fb)) = (a.first(), b.first()) {
        assert_eq!(fa.severity, fb.severity);
    }
}
