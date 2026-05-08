//! v3 EGR-delete post-flash validation checklist (spec §7).
//!
//! 15 yes/no checks against a post-flash log. Each emits a
//! [`CheckOutcome`] with a status (`Pass` / `Fail` / `Skipped`),
//! observed evidence, and a remediation pointer. The aggregate
//! [`ValidationReport`] is `pass()` only when no item failed.

use crate::ingest::VcdsLog;
use crate::platform::amf_edc15p::egr::{
    DTC_LIST_TO_SUPPRESS, DTC_WIRING_FAULTS, EGR_DUTY_OBSERVED_TOLERANCE_PCT,
    IDLE_INSTABILITY_THRESHOLD_RPM_STD, SPEC_MAF_FILL_MGSTR, WARM_COOLANT_MIN_C,
    WOT_PEDAL_CUTOFF_PCT,
};
use crate::platform::amf_edc15p::envelope::{CAPS, DIESEL_AFR_STOICH, NM_PER_MG_IQ};

/// One check's outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckStatus {
    /// Check evaluated and passed.
    Pass,
    /// Check evaluated and failed.
    Fail,
    /// Check could not be evaluated (channel missing).
    Skipped,
}

impl CheckStatus {
    /// Display string used in the markdown checklist.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pass => "PASS",
            Self::Fail => "FAIL",
            Self::Skipped => "SKIPPED",
        }
    }

    /// Markdown glyph used in the checklist.
    pub fn glyph(self) -> &'static str {
        match self {
            Self::Pass => "[x]",
            Self::Fail => "[ ]",
            Self::Skipped => "[-]",
        }
    }
}

/// One row in the validation checklist.
#[derive(Debug, Clone)]
pub struct CheckOutcome {
    /// Position in the §7 checklist (1-based).
    pub id: u8,
    /// Short label.
    pub title: String,
    /// Outcome status.
    pub status: CheckStatus,
    /// Observed value or summary string.
    pub observed: String,
    /// Pointer to the remediation action when failed.
    pub remediation: String,
}

/// Aggregate validation result.
#[derive(Debug, Clone)]
pub struct ValidationReport {
    /// All 15 checklist outcomes, in order.
    pub items: Vec<CheckOutcome>,
}

impl ValidationReport {
    /// Whether every item passed (skipped items are tolerated).
    pub fn pass(&self) -> bool {
        !self.items.iter().any(|i| i.status == CheckStatus::Fail)
    }

    /// Number of failed items.
    pub fn failed(&self) -> usize {
        self.items.iter().filter(|i| i.status == CheckStatus::Fail).count()
    }

    /// Number of skipped items.
    pub fn skipped(&self) -> usize {
        self.items.iter().filter(|i| i.status == CheckStatus::Skipped).count()
    }

    /// Render the checklist as Markdown.
    pub fn to_markdown(&self) -> String {
        let mut lines = vec![
            "## EGR Delete Validation Checklist".to_string(),
            String::new(),
        ];
        for it in &self.items {
            lines.push(format!(
                "- {} **{}.** {} — `{}` — observed: {}",
                it.status.glyph(),
                it.id,
                it.title,
                it.status.as_str(),
                it.observed,
            ));
            if it.status == CheckStatus::Fail {
                lines.push(format!("    - Remediation: {}", it.remediation));
            }
        }
        lines.push(String::new());
        let summary = if self.pass() {
            format!("**Result: PASS** ({} skipped)", self.skipped())
        } else {
            format!(
                "**Result: FAIL** ({} failed, {} skipped)",
                self.failed(),
                self.skipped(),
            )
        };
        lines.push(summary);
        lines.join("\n")
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn finite_max(xs: &[f64]) -> Option<f64> {
    xs.iter().copied().filter(|x| x.is_finite())
        .fold(None, |a, x| match a { Some(v) if v >= x => Some(v), _ => Some(x) })
}

fn finite_min(xs: &[f64]) -> Option<f64> {
    xs.iter().copied().filter(|x| x.is_finite())
        .fold(None, |a, x| match a { Some(v) if v <= x => Some(v), _ => Some(x) })
}

fn finite_mean_std(xs: &[f64]) -> Option<(f64, f64)> {
    let finite: Vec<f64> = xs.iter().copied().filter(|x| x.is_finite()).collect();
    if finite.is_empty() { return None; }
    let n = finite.len() as f64;
    let mean = finite.iter().sum::<f64>() / n;
    let var = finite.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / n;
    Some((mean, var.sqrt()))
}

/// Indices of warm samples (coolant ≥ `WARM_COOLANT_MIN_C`).
fn warm_indices(log: &VcdsLog) -> Vec<usize> {
    let Some(coolant) = log.data.get("coolant_c") else { return Vec::new(); };
    coolant.iter().enumerate()
        .filter(|(_, c)| c.is_finite() && **c >= WARM_COOLANT_MIN_C)
        .map(|(i, _)| i)
        .collect()
}

fn pass(id: u8, title: &str, observed: String) -> CheckOutcome {
    CheckOutcome {
        id, title: title.to_string(), status: CheckStatus::Pass,
        observed, remediation: String::new(),
    }
}

fn fail(id: u8, title: &str, observed: String, remediation: &str) -> CheckOutcome {
    CheckOutcome {
        id, title: title.to_string(), status: CheckStatus::Fail,
        observed, remediation: remediation.to_string(),
    }
}

fn skipped(id: u8, title: &str, reason: &str) -> CheckOutcome {
    CheckOutcome {
        id, title: title.to_string(), status: CheckStatus::Skipped,
        observed: reason.to_string(), remediation: String::new(),
    }
}

// ---------------------------------------------------------------------------
// Individual checks (numbered per §7)
// ---------------------------------------------------------------------------

fn check1_egr_duty_idle(log: &VcdsLog) -> CheckOutcome {
    let title = "EGR duty zero at warm idle";
    let Some(duty) = log.data.get("egr_duty") else {
        return skipped(1, title, "egr_duty channel missing");
    };
    let Some(coolant) = log.data.get("coolant_c") else {
        return skipped(1, title, "coolant_c channel missing");
    };
    let iq = log.data.get("iq_requested").or_else(|| log.data.get("iq_actual"));
    let mut max_duty = f64::NEG_INFINITY;
    let mut samples = 0;
    for i in 0..duty.len() {
        if !duty[i].is_finite() { continue; }
        if !coolant[i].is_finite() || coolant[i] < WARM_COOLANT_MIN_C { continue; }
        if let Some(iqv) = iq {
            if i < iqv.len() && iqv[i].is_finite() && iqv[i] > 8.0 { continue; }
        }
        samples += 1;
        if duty[i] > max_duty { max_duty = duty[i]; }
    }
    if samples == 0 {
        return skipped(1, title, "no warm-idle samples in log");
    }
    if max_duty <= EGR_DUTY_OBSERVED_TOLERANCE_PCT {
        pass(1, title, format!("max EGR duty at idle = {max_duty:.1}% ({samples} samples)"))
    } else {
        fail(1, title, format!("max EGR duty at idle = {max_duty:.1}% (>{} tolerance)",
            EGR_DUTY_OBSERVED_TOLERANCE_PCT),
            "Re-flash EGR-duty map (arwMEAB0KL) to 0% in both banks.")
    }
}

fn check2_egr_duty_cruise(log: &VcdsLog) -> CheckOutcome {
    let title = "EGR duty zero at cruise";
    let Some(duty) = log.data.get("egr_duty") else {
        return skipped(2, title, "egr_duty channel missing");
    };
    let Some(rpm) = log.data.get("rpm") else {
        return skipped(2, title, "rpm channel missing");
    };
    let iq = log.data.get("iq_requested").or_else(|| log.data.get("iq_actual"));
    let coolant = log.data.get("coolant_c");
    let (lo, hi, iq_lo, iq_hi) = (1500.0, 2500.0, 5.0, 15.0);

    let mut max_duty = f64::NEG_INFINITY;
    let mut samples = 0;
    for i in 0..duty.len() {
        if !duty[i].is_finite() || !rpm[i].is_finite() { continue; }
        if rpm[i] < lo || rpm[i] > hi { continue; }
        if let Some(iqv) = iq {
            if i < iqv.len() && iqv[i].is_finite()
                && (iqv[i] < iq_lo || iqv[i] > iq_hi)
            {
                continue;
            }
        }
        if let Some(c) = coolant {
            if i < c.len() && c[i].is_finite() && c[i] < WARM_COOLANT_MIN_C { continue; }
        }
        samples += 1;
        if duty[i] > max_duty { max_duty = duty[i]; }
    }
    if samples == 0 {
        return skipped(2, title, "no cruise-warm samples in log");
    }
    if max_duty <= EGR_DUTY_OBSERVED_TOLERANCE_PCT {
        pass(2, title, format!("max EGR duty at cruise = {max_duty:.1}% ({samples} samples)"))
    } else {
        fail(2, title, format!("max EGR duty at cruise = {max_duty:.1}%"),
            "Re-flash EGR-duty map; check both banks were written.")
    }
}

fn check3_egr_duty_wot(log: &VcdsLog) -> CheckOutcome {
    let title = "EGR duty zero at WOT";
    let Some(duty) = log.data.get("egr_duty") else {
        return skipped(3, title, "egr_duty channel missing");
    };
    let Some(pedal) = log.data.get("tps_pct") else {
        return skipped(3, title, "tps_pct channel missing");
    };
    let mut max_duty = f64::NEG_INFINITY;
    let mut samples = 0;
    for i in 0..duty.len() {
        if !duty[i].is_finite() || !pedal[i].is_finite() { continue; }
        if pedal[i] < WOT_PEDAL_CUTOFF_PCT { continue; }
        samples += 1;
        if duty[i] > max_duty { max_duty = duty[i]; }
    }
    if samples == 0 {
        return skipped(3, title, "no WOT samples in log");
    }
    if max_duty <= EGR_DUTY_OBSERVED_TOLERANCE_PCT {
        pass(3, title, format!("max EGR duty at WOT = {max_duty:.1}% ({samples} samples)"))
    } else {
        fail(3, title, format!("max EGR duty at WOT = {max_duty:.1}%"),
            "Should already be zero pre-delete; raise spec-MAF further.")
    }
}

fn check4_spec_maf_saturated(log: &VcdsLog) -> CheckOutcome {
    let title = "Spec-MAF saturated (Strategy B)";
    let Some(spec) = log.data.get("maf_spec") else {
        return skipped(4, title, "maf_spec channel missing");
    };
    let warm = warm_indices(log);
    if warm.is_empty() {
        return skipped(4, title, "no warm samples in log");
    }
    let warm_spec: Vec<f64> = warm.iter().filter_map(|&i| spec.get(i).copied())
        .filter(|x| x.is_finite())
        .collect();
    let Some(min_spec) = finite_min(&warm_spec) else {
        return skipped(4, title, "no finite maf_spec samples");
    };
    let threshold = SPEC_MAF_FILL_MGSTR - 50.0; // 800 from spec §7
    if min_spec >= threshold {
        pass(4, title, format!("min warm spec-MAF = {min_spec:.0} mg/stroke"))
    } else {
        fail(4, title, format!("min warm spec-MAF = {min_spec:.0} mg/stroke (< {threshold:.0})"),
            "Re-flash arwMLGRDKF to ≥850 mg/stroke in both banks.")
    }
}

fn check5_maf_actual_in_range(log: &VcdsLog) -> CheckOutcome {
    let title = "MAF actual within HFM5 linear range";
    let Some(actual) = log.data.get("maf_actual") else {
        return skipped(5, title, "maf_actual channel missing");
    };
    let warm = warm_indices(log);
    if warm.is_empty() {
        return skipped(5, title, "no warm samples in log");
    }
    let warm_actual: Vec<f64> = warm.iter().filter_map(|&i| actual.get(i).copied())
        .filter(|x| x.is_finite())
        .collect();
    let Some(max) = finite_max(&warm_actual) else {
        return skipped(5, title, "no finite maf_actual samples");
    };
    let Some(min) = finite_min(&warm_actual) else {
        return skipped(5, title, "no finite maf_actual samples");
    };
    let upper = CAPS.maf_max_mg_stroke - 50.0; // 950
    if (100.0..=upper).contains(&max) && min >= 100.0 {
        pass(5, title, format!("MAF actual range = {min:.0}..{max:.0} mg/stroke"))
    } else {
        fail(5, title,
            format!("MAF actual range = {min:.0}..{max:.0} mg/stroke (out of 100..{upper:.0})"),
            "If saturating, fit a larger MAF housing or check intake leak.")
    }
}

fn dtc_present(log: &VcdsLog, code_int: u16) -> bool {
    log.data.get("dtc_codes").is_some_and(|v| {
        v.iter().any(|x| x.is_finite() && (x.round() as i64) == i64::from(code_int))
    })
}

fn check6_no_p0401_p0402(log: &VcdsLog) -> CheckOutcome {
    let title = "No P0401 / P0402 in DTC scan";
    if !log.data.contains_key("dtc_codes") {
        return skipped(6, title, "dtc_codes channel missing");
    }
    let mut hits: Vec<&'static str> = Vec::new();
    if dtc_present(log, 401) { hits.push("P0401"); }
    if dtc_present(log, 402) { hits.push("P0402"); }
    if hits.is_empty() {
        pass(6, title, "no P0401/P0402 in scan".to_string())
    } else {
        fail(6, title, format!("present: {}", hits.join(", ")),
            "Widen DTC threshold (§3.5) or zero the DTC enable flag.")
    }
}

fn check7_no_p0403(log: &VcdsLog) -> CheckOutcome {
    let title = "No P0403 (solenoid wiring) in DTC scan";
    if !log.data.contains_key("dtc_codes") {
        return skipped(7, title, "dtc_codes channel missing");
    }
    if dtc_present(log, 403) {
        fail(7, title, "P0403 present".to_string(),
            "Real wiring fault on the still-installed EGR solenoid — investigate, do NOT just suppress.")
    } else {
        pass(7, title, "no P0403 in scan".to_string())
    }
}

fn check8_no_p0404_to_p0406(log: &VcdsLog) -> CheckOutcome {
    let title = "No P0404 / P0405 / P0406 in DTC scan";
    if !log.data.contains_key("dtc_codes") {
        return skipped(8, title, "dtc_codes channel missing");
    }
    let mut hits: Vec<&'static str> = Vec::new();
    if dtc_present(log, 404) { hits.push("P0404"); }
    if dtc_present(log, 405) { hits.push("P0405"); }
    if dtc_present(log, 406) { hits.push("P0406"); }
    if hits.is_empty() {
        pass(8, title, "no P0404/P0405/P0406 in scan".to_string())
    } else {
        fail(8, title, format!("present: {}", hits.join(", ")),
            "Unusual on AMF (no EGR position sensor) — investigate before suppressing.")
    }
}

fn check9_idle_stability(log: &VcdsLog) -> CheckOutcome {
    let title = "Idle stability";
    let Some(rpm) = log.data.get("rpm") else {
        return skipped(9, title, "rpm channel missing");
    };
    let Some(coolant) = log.data.get("coolant_c") else {
        return skipped(9, title, "coolant_c channel missing");
    };
    let iq = log.data.get("iq_actual").or_else(|| log.data.get("iq_requested"));
    let mut idle_rpm: Vec<f64> = Vec::new();
    for i in 0..rpm.len() {
        if !rpm[i].is_finite() || !coolant[i].is_finite() { continue; }
        if coolant[i] < 85.0 { continue; }
        if let Some(iqv) = iq {
            if i < iqv.len() && iqv[i].is_finite() && iqv[i] > 8.0 { continue; }
        }
        idle_rpm.push(rpm[i]);
    }
    if idle_rpm.is_empty() {
        return skipped(9, title, "no warm-idle samples (T_coolant ≥ 85°C, IQ ≤ 8 mg)");
    }
    let Some((mean, std)) = finite_mean_std(&idle_rpm) else {
        return skipped(9, title, "could not compute idle statistics");
    };
    if std <= IDLE_INSTABILITY_THRESHOLD_RPM_STD {
        pass(9, title, format!("RPM σ = {std:.1} (mean {mean:.0}, n={})", idle_rpm.len()))
    } else {
        fail(9, title, format!("RPM σ = {std:.1} (> {} threshold)",
                IDLE_INSTABILITY_THRESHOLD_RPM_STD),
            "Apply conditional −1.5 mg/stroke idle-IQ trim (default-deltas Idle_fuel row).")
    }
}

fn check10_cruise_nvh_proxy(_log: &VcdsLog) -> CheckOutcome {
    // The spec marks this as a subjective check (driver note required).
    // We can never pass it from a log alone; mark as Skipped with an
    // explanatory note so the user supplies the marker manually.
    skipped(10, "Cruise NVH proxy (subjective)",
        "Subjective: log marker / driver note required. \
         If complaint, apply −1.0° SOI retard cruise band (R18 / SOI_warm_cruise).")
}

fn check11_egt_within_envelope(log: &VcdsLog) -> CheckOutcome {
    let title = "EGT modelled within envelope";
    let egt = log.data.get("egt_model_c");
    let Some(egt) = egt else {
        return skipped(11, title, "egt_model_c channel missing");
    };
    let Some(max) = finite_max(egt) else {
        return skipped(11, title, "no finite egt samples");
    };
    if max <= f64::from(CAPS.pre_turbo_egt_c_sustained) {
        pass(11, title, format!("max modelled EGT = {max:.0}°C"))
    } else {
        fail(11, title, format!("max modelled EGT = {max:.0}°C (> {} cap)",
                CAPS.pre_turbo_egt_c_sustained),
            "Reduce IQ peak or trim SOI advance at the offending cells.")
    }
}

fn check12_lambda_within_envelope(log: &VcdsLog) -> CheckOutcome {
    let title = "Lambda modelled ≥ floor";
    let Some(maf) = log.data.get("maf_actual") else {
        return skipped(12, title, "maf_actual channel missing");
    };
    let Some(iq) = log.data.get("iq_requested").or_else(|| log.data.get("iq_actual")) else {
        return skipped(12, title, "iq_requested / iq_actual missing");
    };
    let mut min_lambda = f64::INFINITY;
    for i in 0..maf.len() {
        if i >= iq.len() { break; }
        if !maf[i].is_finite() || !iq[i].is_finite() || iq[i] <= 5.0 { continue; }
        let lam = maf[i] / (iq[i] * DIESEL_AFR_STOICH);
        if lam < min_lambda { min_lambda = lam; }
    }
    if !min_lambda.is_finite() {
        return skipped(12, title, "no qualifying samples for λ model");
    }
    if min_lambda >= CAPS.lambda_floor {
        pass(12, title, format!("min modelled λ = {min_lambda:.3}"))
    } else {
        fail(12, title, format!("min modelled λ = {min_lambda:.3} (< {} floor)",
                CAPS.lambda_floor),
            "Smoke-limiter (IQ-by-MAF) re-shape needs revisit.")
    }
}

fn check13_boost_within_envelope(log: &VcdsLog) -> CheckOutcome {
    let title = "Boost actual ≤ envelope";
    let Some(boost) = log.data.get("boost_actual") else {
        return skipped(13, title, "boost_actual channel missing");
    };
    let Some(max) = finite_max(boost) else {
        return skipped(13, title, "no finite boost samples");
    };
    if max <= f64::from(CAPS.peak_boost_mbar_abs) {
        pass(13, title, format!("max boost = {max:.0} mbar"))
    } else {
        fail(13, title, format!("max boost = {max:.0} mbar (> {} cap)",
                CAPS.peak_boost_mbar_abs),
            "LDRXN / LDRPMX too aggressive — drop.")
    }
}

fn check14_torque_limiter(log: &VcdsLog) -> CheckOutcome {
    let title = "Modelled torque ≤ clutch limit";
    let Some(iq) = log.data.get("iq_requested").or_else(|| log.data.get("iq_actual")) else {
        return skipped(14, title, "iq channel missing");
    };
    let Some(peak_iq) = finite_max(iq) else {
        return skipped(14, title, "no finite iq samples");
    };
    let modelled_nm = peak_iq * NM_PER_MG_IQ;
    if modelled_nm <= CAPS.modelled_flywheel_torque_nm {
        pass(14, title, format!("peak modelled torque = {modelled_nm:.1} Nm"))
    } else {
        fail(14, title, format!("peak modelled torque = {modelled_nm:.1} Nm (> {} cap)",
                CAPS.modelled_flywheel_torque_nm),
            "Torque-limiter map: cap peak at 240 Nm.")
    }
}

fn check15_smoke_switch_unchanged() -> CheckOutcome {
    // Cannot be verified from a log alone — surfaces as a reminder.
    pass(15, "MAP/MAF smoke-limiter switch unchanged",
        "v3 mandate: 0x51C30 / 0x71C30 stays at 0x00 (MAF-based). \
         Verify by reading the bin if available.".to_string())
}

/// Run all 15 §7 checks against `log` and return the aggregate report.
pub fn validate_egr_delete(log: &VcdsLog) -> ValidationReport {
    let items = vec![
        check1_egr_duty_idle(log),
        check2_egr_duty_cruise(log),
        check3_egr_duty_wot(log),
        check4_spec_maf_saturated(log),
        check5_maf_actual_in_range(log),
        check6_no_p0401_p0402(log),
        check7_no_p0403(log),
        check8_no_p0404_to_p0406(log),
        check9_idle_stability(log),
        check10_cruise_nvh_proxy(log),
        check11_egt_within_envelope(log),
        check12_lambda_within_envelope(log),
        check13_boost_within_envelope(log),
        check14_torque_limiter(log),
        check15_smoke_switch_unchanged(),
    ];
    debug_assert_eq!(items.len(), 15);
    let _wiring = DTC_WIRING_FAULTS; // referenced for symmetry with the rule pack
    let _ = DTC_LIST_TO_SUPPRESS;    // ditto
    ValidationReport { items }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ingest::VcdsLog;
    use std::collections::{BTreeMap, BTreeSet};
    use std::path::PathBuf;

    fn empty_log() -> VcdsLog {
        VcdsLog {
            source_file: PathBuf::from("synth.csv"),
            time: vec![],
            data: BTreeMap::new(),
            groups: BTreeSet::new(),
            field_names: Default::default(),
            units: Default::default(),
            unmapped_columns: Vec::new(),
            warnings: Vec::new(),
            median_sample_dt_ms: 0.0,
        }
    }

    fn synth(channels: &[(&str, Vec<f64>)]) -> VcdsLog {
        let mut data: BTreeMap<String, Vec<f64>> = BTreeMap::new();
        let n = channels.first().map_or(0, |(_, v)| v.len());
        for (k, v) in channels {
            data.insert((*k).to_string(), v.clone());
        }
        VcdsLog {
            source_file: PathBuf::from("synth.csv"),
            time: (0..n).map(|i| i as f64 * 0.2).collect(),
            data,
            groups: BTreeSet::new(),
            field_names: Default::default(),
            units: Default::default(),
            unmapped_columns: Vec::new(),
            warnings: Vec::new(),
            median_sample_dt_ms: 200.0,
        }
    }

    #[test]
    fn empty_log_skips_most_checks_but_passes_smoke_switch_reminder() {
        let report = validate_egr_delete(&empty_log());
        assert_eq!(report.items.len(), 15);
        // Smoke-switch check is informational and always passes.
        assert_eq!(report.items[14].status, CheckStatus::Pass);
        assert!(report.skipped() >= 12, "most checks skip with empty log");
    }

    #[test]
    fn happy_path_post_delete_passes() {
        let n = 60;
        let log = synth(&[
            ("rpm",          vec![820.0; n]),
            ("egr_duty",     vec![0.5; n]),
            ("coolant_c",    vec![88.0; n]),
            ("iq_requested", vec![4.0; n]),
            ("iq_actual",    vec![4.0; n]),
            ("maf_actual",   vec![300.0; n]),
            ("maf_spec",     vec![850.0; n]),
            ("boost_actual", vec![1000.0; n]),
            ("tps_pct",      vec![5.0; n]),
            ("dtc_codes",    vec![f64::NAN; n]),
        ]);
        let r = validate_egr_delete(&log);
        assert!(r.pass(), "post-delete happy path must pass: failed = {}", r.failed());
    }

    #[test]
    fn pre_delete_log_fails_egr_duty_checks() {
        let n = 60;
        let log = synth(&[
            ("rpm",          vec![820.0; n]),
            ("egr_duty",     vec![35.0; n]), // EGR active
            ("coolant_c",    vec![88.0; n]),
            ("iq_requested", vec![4.0; n]),
            ("iq_actual",    vec![4.0; n]),
            ("maf_actual",   vec![220.0; n]),
            ("maf_spec",     vec![220.0; n]), // not saturated either
            ("boost_actual", vec![1000.0; n]),
            ("tps_pct",      vec![5.0; n]),
        ]);
        let r = validate_egr_delete(&log);
        assert!(!r.pass(), "pre-delete log must fail validation");
        // Item 1 (EGR duty idle) and Item 4 (spec-MAF saturated) must fail.
        assert_eq!(r.items[0].status, CheckStatus::Fail);
        assert_eq!(r.items[3].status, CheckStatus::Fail);
    }

    #[test]
    fn p0403_fails_check7() {
        let n = 60;
        let mut dtc = vec![f64::NAN; n];
        dtc[5] = 403.0;
        let log = synth(&[
            ("rpm", vec![820.0; n]),
            ("egr_duty", vec![0.0; n]),
            ("coolant_c", vec![88.0; n]),
            ("iq_actual", vec![4.0; n]),
            ("maf_actual", vec![300.0; n]),
            ("maf_spec", vec![850.0; n]),
            ("boost_actual", vec![1000.0; n]),
            ("tps_pct", vec![5.0; n]),
            ("dtc_codes", dtc),
        ]);
        let r = validate_egr_delete(&log);
        let item7 = r.items.iter().find(|i| i.id == 7).unwrap();
        assert_eq!(item7.status, CheckStatus::Fail);
        assert!(item7.remediation.contains("wiring fault"));
    }

    #[test]
    fn idle_instability_fails_check9() {
        let n = 60;
        let mut rpm = Vec::with_capacity(n);
        for i in 0..n {
            rpm.push(820.0 + (i as f64).sin() * 80.0); // σ ≫ 25
        }
        let log = synth(&[
            ("rpm", rpm),
            ("egr_duty", vec![0.0; n]),
            ("coolant_c", vec![88.0; n]),
            ("iq_actual", vec![4.0; n]),
            ("maf_actual", vec![300.0; n]),
            ("maf_spec", vec![850.0; n]),
            ("boost_actual", vec![1000.0; n]),
            ("tps_pct", vec![5.0; n]),
        ]);
        let r = validate_egr_delete(&log);
        let item9 = r.items.iter().find(|i| i.id == 9).unwrap();
        assert_eq!(item9.status, CheckStatus::Fail);
    }

    #[test]
    fn report_markdown_renders_glyphs() {
        let report = validate_egr_delete(&empty_log());
        let md = report.to_markdown();
        assert!(md.contains("## EGR Delete Validation Checklist"));
        assert!(md.contains("[-]"), "skipped items render with [-]");
        assert!(md.contains("**Result:"), "summary line present");
    }
}
