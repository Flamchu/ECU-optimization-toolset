//! Recommendation engine.
//!
//! Joins findings with the default-deltas table and runs every emitted
//! delta through the envelope clamper before emitting it.
//!
//! A recommendation is one of three states:
//!
//! - `APPLY` — delta is sane, falls inside the envelope.
//! - `SKIP` — no rule fired that would justify this delta.
//! - `BLOCKED — envelope cap` — a rule fired AND the proposed delta would
//!   exit the envelope; the cap that fired is named.

use std::collections::BTreeSet;

use crate::platform::amf_edc15p::default_deltas::{DefaultDelta, DeltaKind, DEFAULT_DELTAS};
use crate::platform::amf_edc15p::envelope::{
    clamp_boost_target, clamp_egr_duty_pct, clamp_iq, clamp_soi, clamp_spec_maf,
    clamp_svbl, clamp_torque_nm, ClampOutcome, CAPS,
};
use crate::platform::amf_edc15p::stock_refs::stock_boost_at_rpm;
use crate::rules::base::{Finding, Severity};

/// Outcome state of one recommendation row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// Delta should be applied; falls inside the envelope.
    Apply,
    /// No triggering rule fired; leave the map at stock.
    Skip,
    /// A rule fired but the envelope clamped the proposed value.
    Blocked,
}

impl Status {
    /// Display string used in the Markdown report.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Apply => "APPLY",
            Self::Skip => "SKIP",
            Self::Blocked => "BLOCKED — envelope cap",
        }
    }
}

/// One row in the final action table.
#[derive(Debug, Clone)]
pub struct Recommendation {
    /// Symbolic map this recommendation targets.
    pub map_name: String,
    /// Free-text cell selector.
    pub cell_selector: String,
    /// Outcome state.
    pub status: Status,
    /// Display string for the proposed value (or `"—"` / `"leave stock"`).
    pub proposed_value_text: String,
    /// One-paragraph rationale (may include a `CAP:` suffix when blocked).
    pub rationale: String,
    /// Rule ids that gate this recommendation.
    pub rule_refs: Vec<String>,
    /// Name of the cap that blocked this recommendation, if any.
    pub blocked_cap: Option<String>,
}

fn firing_rules(findings: &[Finding]) -> BTreeSet<String> {
    findings.iter()
        .filter(|f| !f.skipped && f.severity != Severity::Info)
        .map(|f| f.rule_id.to_string())
        .collect()
}

fn formatted(o: &ClampOutcome, original: &str) -> (String, Status, Option<String>, String) {
    if o.blocked {
        (
            format!("BLOCKED — clamped to {:.4}", o.value).replace(".0000", ""),
            Status::Blocked,
            Some(o.cap_name.to_string()),
            o.explanation.clone(),
        )
    } else {
        (original.to_string(), Status::Apply, None, String::new())
    }
}

fn pretty_signed(v: f64, unit: &str) -> String {
    let sign = if v >= 0.0 { "+" } else { "" };
    if v.fract() == 0.0 {
        format!("{sign}{v:.0} {unit}")
    } else {
        format!("{sign}{v} {unit}")
    }
}

fn rationale_with_cap(note: &str, extra: &str) -> String {
    if extra.is_empty() { note.to_string() } else { format!("{note}\n  CAP: {extra}") }
}

fn rule_refs_vec(d: &DefaultDelta) -> Vec<String> {
    d.rule_refs.iter().map(|s| (*s).to_string()).collect()
}

fn eval_default(d: &DefaultDelta, firing: &BTreeSet<String>) -> Recommendation {
    let any_firing = d.rule_refs.iter().any(|r| firing.contains(*r));
    if !d.rule_refs.is_empty() && !any_firing {
        return Recommendation {
            map_name: d.map_name.to_string(),
            cell_selector: d.cell_selector.to_string(),
            status: Status::Skip,
            proposed_value_text: "—".to_string(),
            rationale: format!(
                "No triggering rule fired ({}); leave stock.",
                d.rule_refs.join(", ")
            ),
            rule_refs: rule_refs_vec(d),
            blocked_cap: None,
        };
    }

    match (d.kind, d.value) {
        (DeltaKind::LeaveStock, _) => Recommendation {
            map_name: d.map_name.to_string(),
            cell_selector: d.cell_selector.to_string(),
            status: Status::Skip,
            proposed_value_text: "leave stock".to_string(),
            rationale: d.note.to_string(),
            rule_refs: rule_refs_vec(d),
            blocked_cap: None,
        },
        (DeltaKind::DeltaMbar, Some(value)) => {
            let original = pretty_signed(value, "mbar");
            let rpm_for_clamp = if d.cell_selector.contains("2000-3500") { 3500.0 } else { 4000.0 };
            let proposed_abs = stock_boost_at_rpm(rpm_for_clamp) + value;
            let outcome = clamp_boost_target(proposed_abs, rpm_for_clamp);
            let (text, status, cap, extra) = formatted(&outcome, &original);
            Recommendation {
                map_name: d.map_name.to_string(),
                cell_selector: d.cell_selector.to_string(),
                status, proposed_value_text: text,
                rationale: rationale_with_cap(d.note, &extra),
                rule_refs: rule_refs_vec(d), blocked_cap: cap,
            }
        }
        (DeltaKind::SetTo, Some(value)) => {
            let original = format!("set to {value} mg/stroke");
            let outcome = clamp_iq(value);
            let (text, status, cap, extra) = formatted(&outcome, &original);
            Recommendation {
                map_name: d.map_name.to_string(),
                cell_selector: d.cell_selector.to_string(),
                status, proposed_value_text: text,
                rationale: rationale_with_cap(d.note, &extra),
                rule_refs: rule_refs_vec(d), blocked_cap: cap,
            }
        }
        (DeltaKind::DeltaDeg, Some(value)) => {
            let original = pretty_signed(value, "° BTDC");
            let absolute_soi = 21.0 + value;
            let outcome = clamp_soi(absolute_soi, 45.0);
            let (text, status, cap, extra) = formatted(&outcome, &original);
            Recommendation {
                map_name: d.map_name.to_string(),
                cell_selector: d.cell_selector.to_string(),
                status, proposed_value_text: text,
                rationale: rationale_with_cap(d.note, &extra),
                rule_refs: rule_refs_vec(d), blocked_cap: cap,
            }
        }
        (DeltaKind::DeltaMg, Some(value)) => {
            let original = pretty_signed(value, "mg");
            let outcome = clamp_iq(value);
            let (text, status, cap, extra) = formatted(&outcome, &original);
            Recommendation {
                map_name: d.map_name.to_string(),
                cell_selector: d.cell_selector.to_string(),
                status, proposed_value_text: text,
                rationale: rationale_with_cap(d.note, &extra),
                rule_refs: rule_refs_vec(d), blocked_cap: cap,
            }
        }
        (DeltaKind::ClampPeak, value) => {
            if d.map_name == "Torque_Limiter" {
                if let Some(v) = value {
                    let outcome = clamp_torque_nm(v);
                    let (text, status, cap, extra) = formatted(
                        &outcome, &format!("clamp peak ≤ {v} Nm"),
                    );
                    return Recommendation {
                        map_name: d.map_name.to_string(),
                        cell_selector: d.cell_selector.to_string(),
                        status, proposed_value_text: text,
                        rationale: rationale_with_cap(d.note, &extra),
                        rule_refs: rule_refs_vec(d), blocked_cap: cap,
                    };
                }
            }
            // Smoke limiter rows: the clamp is conceptual (enforce λ ≥ 1.20)
            // rather than a single number.
            Recommendation {
                map_name: d.map_name.to_string(),
                cell_selector: d.cell_selector.to_string(),
                status: Status::Apply,
                proposed_value_text: format!("enforce λ ≥ {}", CAPS.lambda_floor),
                rationale: d.note.to_string(),
                rule_refs: rule_refs_vec(d), blocked_cap: None,
            }
        }
        (DeltaKind::ExtendAxis, Some(value)) => {
            let outcome = clamp_iq(value);
            let original = format!("extend axis to {value} mg");
            let (text, status, cap, extra) = formatted(&outcome, &original);
            Recommendation {
                map_name: d.map_name.to_string(),
                cell_selector: d.cell_selector.to_string(),
                status, proposed_value_text: text,
                rationale: rationale_with_cap(d.note, &extra),
                rule_refs: rule_refs_vec(d), blocked_cap: cap,
            }
        }
        (DeltaKind::ZeroEgr, _) => {
            // Mandatory v3: always emit APPLY. Run the proposed 0 % through
            // clamp_egr_duty_pct so any future caller passing a non-zero
            // value would still be blocked.
            let outcome = clamp_egr_duty_pct(0.0);
            let (text, status, cap, extra) = formatted(&outcome, "set all cells to 0 % duty");
            Recommendation {
                map_name: d.map_name.to_string(),
                cell_selector: d.cell_selector.to_string(),
                status, proposed_value_text: text,
                rationale: rationale_with_cap(d.note, &extra),
                rule_refs: rule_refs_vec(d), blocked_cap: cap,
            }
        }
        (DeltaKind::FillSpecMaf, value) => {
            let fill = value.unwrap_or(CAPS.spec_maf_fill_mg_stroke);
            let outcome = clamp_spec_maf(fill);
            let (text, status, cap, extra) = formatted(
                &outcome, &format!("fill all cells with {fill:.0} mg/stroke"),
            );
            Recommendation {
                map_name: d.map_name.to_string(),
                cell_selector: d.cell_selector.to_string(),
                status, proposed_value_text: text,
                rationale: rationale_with_cap(d.note, &extra),
                rule_refs: rule_refs_vec(d), blocked_cap: cap,
            }
        }
        (DeltaKind::SuppressDtc, _) => {
            // Symbolic only — no numeric clamp. Always APPLY in v3.
            Recommendation {
                map_name: d.map_name.to_string(),
                cell_selector: d.cell_selector.to_string(),
                status: Status::Apply,
                proposed_value_text: "widen thresholds / disable enable-flag".to_string(),
                rationale: d.note.to_string(),
                rule_refs: rule_refs_vec(d),
                blocked_cap: None,
            }
        }
        _ => Recommendation {
            map_name: d.map_name.to_string(),
            cell_selector: d.cell_selector.to_string(),
            status: Status::Skip,
            proposed_value_text: "—".to_string(),
            rationale: format!("unsupported delta kind: {:?}", d.kind),
            rule_refs: rule_refs_vec(d), blocked_cap: None,
        },
    }
}

/// Produce one [`Recommendation`] per row in the default-deltas table.
pub fn recommend(findings: &[Finding]) -> Vec<Recommendation> {
    let firing = firing_rules(findings);
    let mut out = Vec::with_capacity(DEFAULT_DELTAS.len());
    for d in DEFAULT_DELTAS {
        if d.map_name == "SVBL" {
            let outcome = clamp_svbl(0.0);
            let rationale = if outcome.blocked {
                outcome.explanation
            } else {
                d.note.to_string()
            };
            out.push(Recommendation {
                map_name: "SVBL".to_string(),
                cell_selector: "scalar".to_string(),
                status: Status::Skip,
                proposed_value_text: "leave stock".to_string(),
                rationale,
                rule_refs: Vec::new(),
                blocked_cap: None,
            });
            continue;
        }
        out.push(eval_default(d, &firing));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::base::Severity;

    fn finding(rule_id: &'static str, sev: Severity) -> Finding {
        Finding {
            rule_id, severity: sev, pull_id: 1,
            t_start: 0.0, t_end: 1.0,
            observed_extreme: 0.0, threshold: 0.0,
            rationale: String::new(), recommended_action_ref: None,
            skipped: false,
        }
    }

    #[test]
    fn no_findings_skips_conditional_rows_but_applies_egr_mandate() {
        let recs = recommend(&[]);
        // Rows with rule_refs need a triggering rule → all SKIP without findings.
        for r in &recs {
            if !r.rule_refs.is_empty() {
                assert!(matches!(r.status, Status::Skip),
                    "{} should SKIP without firing rule", r.map_name);
            }
        }
        // The unconditional v3 EGR-delete mandate ALWAYS applies, regardless
        // of findings (it is the v3 thesis).
        let egr = recs.iter().find(|r| r.map_name == "AGR_arwMEAB0KL").unwrap();
        assert_eq!(egr.status, Status::Apply);
        let spec = recs.iter().find(|r| r.map_name == "arwMLGRDKF").unwrap();
        assert_eq!(spec.status, Status::Apply);
        let dtc = recs.iter().find(|r| r.map_name == "DTC_thresholds").unwrap();
        assert_eq!(dtc.status, Status::Apply);
        // The MAF/MAP switch is explicitly LEAVE STOCK regardless of evidence.
        let sw = recs.iter().find(|r| r.map_name == "MAF_MAP_smoke_switch").unwrap();
        assert_eq!(sw.status, Status::Skip);
        assert_eq!(sw.proposed_value_text, "leave stock");
    }

    #[test]
    fn r07_firing_promotes_driver_wish_to_apply() {
        let recs = recommend(&[finding("R07", Severity::Critical)]);
        let dw = recs.iter().find(|r| r.map_name == "Driver_Wish").unwrap();
        assert_eq!(dw.status, Status::Apply);
    }

    #[test]
    fn r08_firing_clamps_torque_limiter() {
        let recs = recommend(&[finding("R08", Severity::Critical)]);
        let t = recs.iter().find(|r| r.map_name == "Torque_Limiter").unwrap();
        assert_eq!(t.status, Status::Apply);
        assert!(t.proposed_value_text.contains("240"));
    }

    #[test]
    fn r09_firing_emits_safe_soi_delta() {
        let recs = recommend(&[finding("R09", Severity::Critical)]);
        let soi = recs.iter().find(|r| r.map_name == "SOI").unwrap();
        assert_eq!(soi.status, Status::Apply);
    }

    #[test]
    fn svbl_always_leaves_stock() {
        let recs = recommend(&[finding("R02", Severity::Critical)]);
        let svbl = recs.iter().find(|r| r.map_name == "SVBL").unwrap();
        assert_eq!(svbl.status, Status::Skip);
        assert_eq!(svbl.proposed_value_text, "leave stock");
    }
}
