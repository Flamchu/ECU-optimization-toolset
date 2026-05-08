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
    clamp_boost_target, clamp_egr_duty_pct, clamp_fan_on_c, clamp_fan_run_on_s, clamp_iq,
    clamp_low_pedal_slope, clamp_soi, clamp_spec_maf, clamp_svbl, clamp_torque_nm,
    ClampOutcome, CAPS,
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
            // Use the SOI threshold IQ so the cap actually engages for SOI rows
            // that target high-IQ regions; cruise NVH retard rows pass through.
            let probe_iq = if d.map_name == "SOI_warm_cruise" { 10.0 } else { 45.0 };
            let outcome = clamp_soi(absolute_soi, probe_iq);
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
            // Smoke limiter rows: the clamp is conceptual (enforce λ ≥ 1.05)
            // rather than a single number. Cross-link with CAPS.
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
            // Mandatory: always emit APPLY. Run the proposed 0 % through
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
            // Symbolic only — no numeric clamp. Always APPLY.
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
        (DeltaKind::Flatten, value) => {
            // Conditional driveability row. Only applies when its trigger
            // rule fires; the engine routed us here only after the rule
            // gate passed.
            let target = value.unwrap_or(CAPS.low_pedal_slope_max_mg_per_pct);
            let clamped = clamp_low_pedal_slope(target);
            let action = format!(
                "target slope ≤ {:.2} mg/stroke per pedal-percent across pedal {}-{} % \
                 (idle creep ≤ {} % preserved)",
                clamped,
                CAPS.low_pedal_idle_creep_pct,
                CAPS.low_pedal_band_top_pct,
                CAPS.low_pedal_idle_creep_pct,
            );
            Recommendation {
                map_name: d.map_name.to_string(),
                cell_selector: d.cell_selector.to_string(),
                status: Status::Apply,
                proposed_value_text: action,
                rationale: d.note.to_string(),
                rule_refs: rule_refs_vec(d),
                blocked_cap: None,
            }
        }
        (DeltaKind::FanThresholds, _) => {
            // Unconditional thermal row. Build the four clamped thresholds
            // from CAPS so they audit against the named constants.
            let s1_off = clamp_fan_on_c(88);
            let s1_on  = clamp_fan_on_c(93);
            let s2_off = clamp_fan_on_c(95);
            let s2_on  = clamp_fan_on_c(98); // capped at fan_on_max_c
            let action = format!(
                "stage-1 on/off = {s1_on} / {s1_off} °C; stage-2 on/off = {s2_on} / {s2_off} °C \
                 (hysteresis ≥ {} °C, stage gap ≥ {} °C, all clamped to [{}-{}] °C)",
                CAPS.fan_hysteresis_min_c,
                CAPS.fan_stage_gap_min_c,
                CAPS.fan_on_min_c,
                CAPS.fan_on_max_c,
            );
            Recommendation {
                map_name: d.map_name.to_string(),
                cell_selector: d.cell_selector.to_string(),
                status: Status::Apply,
                proposed_value_text: action,
                rationale: d.note.to_string(),
                rule_refs: rule_refs_vec(d),
                blocked_cap: None,
            }
        }
        (DeltaKind::FanRunOn, value) => {
            let delta = value.unwrap_or(60.0);
            // Treat the delta as additive over a 60-s OEM baseline (the
            // real OEM run-on is firmware-dependent — see open questions).
            let baseline = 60u16;
            let proposed_total = baseline.saturating_add(delta.max(0.0).round() as u16);
            let clamped_total = clamp_fan_run_on_s(proposed_total);
            let actual_delta = clamped_total.saturating_sub(baseline);
            let action = format!(
                "+{actual_delta} s post-key-off run-on (clamped total ≤ {} s)",
                CAPS.fan_run_on_total_max_s,
            );
            Recommendation {
                map_name: d.map_name.to_string(),
                cell_selector: d.cell_selector.to_string(),
                status: Status::Apply,
                proposed_value_text: action,
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
        for r in &recs {
            if !r.rule_refs.is_empty() {
                assert!(matches!(r.status, Status::Skip),
                    "{} should SKIP without firing rule", r.map_name);
            }
        }
        // The unconditional EGR-delete mandate ALWAYS applies.
        let bank_a = recs.iter().find(|r| r.map_name == "AGR_arwMEAB0KL").unwrap();
        assert_eq!(bank_a.status, Status::Apply);
        let bank_b = recs.iter().find(|r| r.map_name == "AGR_arwMEAB1KL").unwrap();
        assert_eq!(bank_b.status, Status::Apply);
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

    #[test]
    fn r21_firing_promotes_idle_fuel_to_apply() {
        let recs = recommend(&[finding("R21", Severity::Warn)]);
        let idle = recs.iter().find(|r| r.map_name == "Idle_fuel").unwrap();
        assert_eq!(idle.status, Status::Apply);
    }

    #[test]
    fn engine_outputs_one_recommendation_per_default_delta_row() {
        // Acceptance: every row in DEFAULT_DELTAS becomes one recommendation row.
        let recs = recommend(&[]);
        assert_eq!(recs.len(), DEFAULT_DELTAS.len());
        assert_eq!(recs.len(), 25);
    }

    #[test]
    fn r22_firing_promotes_low_pedal_flatten_to_apply() {
        let recs = recommend(&[finding("R22", Severity::Warn)]);
        let lp = recs.iter().find(|r| r.map_name == "Driver_Wish_low_pedal").unwrap();
        assert_eq!(lp.status, Status::Apply);
        assert!(lp.proposed_value_text.contains("0.40")
            || lp.proposed_value_text.contains("0.40 mg"));
    }

    #[test]
    fn fan_thresholds_always_apply_with_clamped_values() {
        // Unconditional row: APPLY without any rule firing.
        let recs = recommend(&[]);
        let fan = recs.iter().find(|r| r.map_name == "Fan_thresholds").unwrap();
        assert_eq!(fan.status, Status::Apply);
        // The four numeric thresholds must be in [88, 98] inclusive.
        let s = &fan.proposed_value_text;
        for needle in ["88", "93", "95", "98"] {
            assert!(s.contains(needle),
                "fan-threshold action must mention {needle} in: {s}");
        }
    }

    #[test]
    fn fan_run_on_clamped_under_total_ceiling() {
        let recs = recommend(&[]);
        let r = recs.iter().find(|r| r.map_name == "Fan_run_on").unwrap();
        assert_eq!(r.status, Status::Apply);
        assert!(r.proposed_value_text.contains("60")
            || r.proposed_value_text.contains("90"),
            "fan-run-on action must mention an additive seconds value: {}",
            r.proposed_value_text);
    }

    #[test]
    fn engine_emits_both_egr_banks() {
        // Bank-pair contract: one row per bank.
        let recs = recommend(&[]);
        let banks: Vec<&str> = recs.iter()
            .filter(|r| r.map_name.starts_with("AGR_arwMEAB"))
            .map(|r| r.map_name.as_str()).collect();
        assert!(banks.contains(&"AGR_arwMEAB0KL"));
        assert!(banks.contains(&"AGR_arwMEAB1KL"));
    }

    #[test]
    fn smoke_rows_render_lambda_floor_from_caps() {
        // The rendered λ value is the CAPS const (cross-link audit).
        let recs = recommend(&[finding("R06", Severity::Critical)]);
        let smoke = recs.iter().find(|r| r.map_name == "Smoke_IQ_by_MAF").unwrap();
        assert!(smoke.proposed_value_text.contains(&format!("{}", CAPS.lambda_floor)));
    }
}
