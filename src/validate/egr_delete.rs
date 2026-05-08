//! EGR-delete post-flash validation checklist (spec §10, v4).
//!
//! 15 yes/no checks against a post-flash log. Each emits a
//! [`CheckOutcome`] with a status (`Pass` / `Fail` / `Skipped`),
//! observed evidence, and a remediation pointer. The aggregate
//! [`ValidationReport`] is `pass()` only when no item failed.
//!
//! v4 changes from v3:
//! - DTCs read from `VcdsLog.dtcs` (sidecar), not a synthetic float channel.
//! - Check 10 (cruise NVH) is *always* Skipped — subjective driver note required.
//! - Coolant thresholds split: pull integrity 80 °C; warm cruise/idle 70 °C.
//! - Spec §10 introduces optional pre-delete cross-check items 11/12.

use crate::ingest::VcdsLog;
use crate::platform::amf_edc15p::egr::{
    DTC_GROUP_A, EGR_DUTY_OBSERVED_TOLERANCE_PCT, IDLE_INSTABILITY_THRESHOLD_RPM_STD,
    IDLE_IQ_MAX_MG, IDLE_PEDAL_MAX_PCT, MAF_DEVIATION_FRACTION, WOT_PEDAL_CUTOFF_PCT,
};
use crate::platform::amf_edc15p::envelope::{CAPS, DIESEL_AFR_STOICH};

/// One check's outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckStatus {
    /// Check evaluated and passed.
    Pass,
    /// Check evaluated and failed.
    Fail,
    /// Check could not be evaluated (channel missing) or is manual.
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
    /// Position in the §10 checklist (1-based).
    pub id: u8,
    /// Short label.
    pub title: String,
    /// Outcome status.
    pub status: CheckStatus,
    /// Observed value or summary string.
    pub observed: String,
    /// Pointer to the remediation action when failed (or note when skipped).
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
            if it.status == CheckStatus::Fail || (it.status == CheckStatus::Skipped && !it.remediation.is_empty()) {
                lines.push(format!("    - {}", it.remediation));
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

fn skipped_with_note(id: u8, title: &str, observed: &str, note: &str) -> CheckOutcome {
    CheckOutcome {
        id, title: title.to_string(), status: CheckStatus::Skipped,
        observed: observed.to_string(), remediation: note.to_string(),
    }
}

// ---------------------------------------------------------------------------
// Individual checks (numbered per §10)
// ---------------------------------------------------------------------------

fn check1_idle_maf(log: &VcdsLog) -> CheckOutcome {
    let title = "Idle MAF (post) ≥ 250 mg/str warm";
    let Some(maf) = log.data.get("maf_actual") else {
        return skipped(1, title, "maf_actual channel missing");
    };
    let Some(coolant) = log.data.get("coolant_c") else {
        return skipped(1, title, "coolant_c channel missing");
    };
    let pedal = log.data.get("pedal_pct");
    let iq = log.data.get("iq_requested").or_else(|| log.data.get("iq_actual"));
    let mut min_idle: Option<f64> = None;
    for i in 0..maf.len() {
        if i >= coolant.len() || !coolant[i].is_finite() || coolant[i] < CAPS.warm_coolant_min_c { continue; }
        let pedal_ok = match pedal {
            Some(p) if i < p.len() && p[i].is_finite() => p[i] <= IDLE_PEDAL_MAX_PCT,
            _ => match iq {
                Some(q) if i < q.len() && q[i].is_finite() => q[i] <= IDLE_IQ_MAX_MG,
                _ => true,
            },
        };
        if !pedal_ok || !maf[i].is_finite() { continue; }
        min_idle = Some(min_idle.map_or(maf[i], |m| m.min(maf[i])));
    }
    let Some(min) = min_idle else {
        return skipped(1, title, "no warm-idle MAF samples");
    };
    if min >= 250.0 {
        pass(1, title, format!("min warm-idle MAF = {min:.0} mg/stroke"))
    } else {
        fail(1, title, format!("min warm-idle MAF = {min:.0} mg/stroke (< 250)"),
            "Idle MAF dropped below 250 — confirm both EGR banks zeroed and spec-MAF saturated.")
    }
}

fn check2_cruise_maf(log: &VcdsLog) -> CheckOutcome {
    let title = "Cruise MAF (post) within ±10 % of spec";
    let Some(actual) = log.data.get("maf_actual") else {
        return skipped(2, title, "maf_actual channel missing");
    };
    let Some(spec) = log.data.get("maf_spec") else {
        return skipped(2, title, "maf_spec channel missing");
    };
    let Some(rpm) = log.data.get("rpm") else {
        return skipped(2, title, "rpm channel missing");
    };
    let pedal = log.data.get("pedal_pct");
    let coolant = log.data.get("coolant_c");
    let (lo, hi, iq_lo, iq_hi) = (1500.0, 2500.0, 5.0, 15.0);
    let iq = log.data.get("iq_requested").or_else(|| log.data.get("iq_actual"));

    let mut worst: f64 = 0.0;
    let mut samples = 0;
    for i in 0..actual.len() {
        if i >= spec.len() || i >= rpm.len() { break; }
        if !actual[i].is_finite() || !spec[i].is_finite() || !rpm[i].is_finite() { continue; }
        if !(lo..=hi).contains(&rpm[i]) { continue; }
        if let Some(q) = iq {
            if i < q.len() && q[i].is_finite() && (q[i] < iq_lo || q[i] > iq_hi) { continue; }
        }
        if let Some(c) = coolant {
            if i < c.len() && c[i].is_finite() && c[i] < CAPS.warm_coolant_min_c { continue; }
        }
        if let Some(p) = pedal {
            if i < p.len() && p[i].is_finite() && p[i] >= WOT_PEDAL_CUTOFF_PCT { continue; }
        }
        if spec[i] <= 0.0 { continue; }
        let dev = (actual[i] - spec[i]).abs() / spec[i];
        if dev > worst { worst = dev; }
        samples += 1;
    }
    if samples == 0 {
        return skipped(2, title, "no cruise-warm samples in log");
    }
    if worst <= MAF_DEVIATION_FRACTION {
        pass(2, title, format!("max cruise MAF deviation = {:.1} % ({samples} samples)", worst * 100.0))
    } else {
        fail(2, title, format!("max cruise MAF deviation = {:.1} % (> {:.0} %)",
                worst * 100.0, MAF_DEVIATION_FRACTION * 100.0),
            "Re-flatten arwMLGRDKF (Strategy B fill ≥850).")
    }
}

fn check3_egr_duty_zero(log: &VcdsLog) -> CheckOutcome {
    let title = "EGR duty ≤ 0 % anywhere";
    let Some(duty) = log.data.get("egr_duty") else {
        return skipped(3, title, "egr_duty channel missing");
    };
    let Some(max) = finite_max(duty) else {
        return skipped(3, title, "no finite egr_duty samples");
    };
    if max <= EGR_DUTY_OBSERVED_TOLERANCE_PCT {
        pass(3, title, format!("max EGR duty = {max:.1} %"))
    } else {
        fail(3, title, format!("max EGR duty = {max:.1} %"),
            "Re-flash both banks of AGR_arwMEAB0KL/arwMEAB1KL to 0 %.")
    }
}

fn check4_no_group_a_dtcs(log: &VcdsLog) -> CheckOutcome {
    let title = "No P0401 / P0402 / P0403 in DTC sidecar";
    if log.dtcs.is_empty() {
        return skipped(4, title, "no DTC sidecar provided");
    }
    let hits: Vec<&String> = log.dtcs.iter()
        .filter(|d| DTC_GROUP_A.iter().any(|c| d.eq_ignore_ascii_case(c)))
        .collect();
    if hits.is_empty() {
        pass(4, title, "no Group-A DTCs present".to_string())
    } else {
        let listed = hits.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ");
        fail(4, title, format!("present: {listed}"),
            "Widen DTC threshold (§9 default-deltas DTC_thresholds row); P0403 indicates real wiring fault.")
    }
}

fn check5_idle_stability(log: &VcdsLog) -> CheckOutcome {
    let title = "Idle stability (RPM σ ≤ 25 over warm-idle window)";
    let Some(rpm) = log.data.get("rpm") else {
        return skipped(5, title, "rpm channel missing");
    };
    let Some(coolant) = log.data.get("coolant_c") else {
        return skipped(5, title, "coolant_c channel missing");
    };
    let pedal = log.data.get("pedal_pct");
    let iq = log.data.get("iq_actual").or_else(|| log.data.get("iq_requested"));

    let mut idle_rpm: Vec<f64> = Vec::new();
    for i in 0..rpm.len() {
        if !rpm[i].is_finite() { continue; }
        if i >= coolant.len() || !coolant[i].is_finite() || coolant[i] < CAPS.warm_coolant_min_c {
            continue;
        }
        let idle_ok = match pedal {
            Some(p) if i < p.len() && p[i].is_finite() => p[i] <= IDLE_PEDAL_MAX_PCT,
            _ => match iq {
                Some(q) if i < q.len() && q[i].is_finite() => q[i] <= IDLE_IQ_MAX_MG,
                _ => true,
            },
        };
        if !idle_ok { continue; }
        idle_rpm.push(rpm[i]);
    }
    if idle_rpm.is_empty() {
        return skipped(5, title, "no warm-idle samples");
    }
    let Some((mean, std)) = finite_mean_std(&idle_rpm) else {
        return skipped(5, title, "could not compute idle statistics");
    };
    if std <= IDLE_INSTABILITY_THRESHOLD_RPM_STD {
        pass(5, title, format!("RPM σ = {std:.1} (mean {mean:.0}, n={})", idle_rpm.len()))
    } else {
        fail(5, title, format!("RPM σ = {std:.1} (> {} threshold)",
                IDLE_INSTABILITY_THRESHOLD_RPM_STD),
            "Apply conditional −1.5 mg/stroke idle-IQ trim (Idle_fuel default-delta).")
    }
}

fn check6_lambda_floor(log: &VcdsLog) -> CheckOutcome {
    let title = "Lambda floor never breached at WOT";
    let Some(maf) = log.data.get("maf_actual") else {
        return skipped(6, title, "maf_actual channel missing");
    };
    let Some(iq) = log.data.get("iq_requested").or_else(|| log.data.get("iq_actual")) else {
        return skipped(6, title, "iq_requested / iq_actual missing");
    };
    let pedal = log.data.get("pedal_pct");
    let mut min_lambda = f64::INFINITY;
    for i in 0..maf.len() {
        if i >= iq.len() { break; }
        if !maf[i].is_finite() || !iq[i].is_finite() || iq[i] <= 5.0 { continue; }
        if let Some(p) = pedal {
            if i < p.len() && p[i].is_finite() && p[i] < CAPS.pedal_wot_pct { continue; }
        }
        let lam = maf[i] / (iq[i] * DIESEL_AFR_STOICH);
        if lam < min_lambda { min_lambda = lam; }
    }
    if !min_lambda.is_finite() {
        return skipped(6, title, "no qualifying WOT samples for λ model");
    }
    if min_lambda >= CAPS.lambda_floor {
        pass(6, title, format!("min modelled λ at WOT = {min_lambda:.3}"))
    } else {
        fail(6, title, format!("min modelled λ = {min_lambda:.3} (< {} floor)",
                CAPS.lambda_floor),
            "Smoke_IQ_by_MAF/MAP: raise IQ cap so λ ≥ 1.05.")
    }
}

fn check7_peak_iq(log: &VcdsLog) -> CheckOutcome {
    let title = "Peak IQ ≤ 54 mg in any pull";
    let Some(iq) = log.data.get("iq_requested").or_else(|| log.data.get("iq_actual")) else {
        return skipped(7, title, "iq channel missing");
    };
    let Some(peak) = finite_max(iq) else {
        return skipped(7, title, "no finite iq samples");
    };
    if peak <= CAPS.peak_iq_mg {
        pass(7, title, format!("peak IQ = {peak:.1} mg/stroke"))
    } else {
        fail(7, title, format!("peak IQ = {peak:.1} mg/stroke (> {} cap)", CAPS.peak_iq_mg),
            "Driver_Wish: cap WOT request at 54 mg/stroke.")
    }
}

fn check8_boost_envelope(log: &VcdsLog) -> CheckOutcome {
    let title = "Boost ≤ 2150 mbar abs anywhere";
    let Some(boost) = log.data.get("boost_actual") else {
        return skipped(8, title, "boost_actual channel missing");
    };
    let Some(max) = finite_max(boost) else {
        return skipped(8, title, "no finite boost samples");
    };
    if max <= f64::from(CAPS.peak_boost_mbar_abs) {
        pass(8, title, format!("max boost = {max:.0} mbar abs"))
    } else {
        fail(8, title, format!("max boost = {max:.0} mbar (> {} cap)", CAPS.peak_boost_mbar_abs),
            "LDRXN: lower 2000-3500 rpm cells.")
    }
}

fn check9_soi_envelope(log: &VcdsLog) -> CheckOutcome {
    let title = "SOI ≤ 26° BTDC at IQ ≥ 30 mg";
    let Some(soi) = log.data.get("soi_actual") else {
        return skipped(9, title, "soi_actual channel missing");
    };
    let Some(iq) = log.data.get("iq_requested").or_else(|| log.data.get("iq_actual")) else {
        return skipped(9, title, "iq channel missing");
    };
    let mut worst = f64::NEG_INFINITY;
    for i in 0..soi.len() {
        if i >= iq.len() { break; }
        if !soi[i].is_finite() || !iq[i].is_finite() { continue; }
        if iq[i] >= CAPS.soi_iq_threshold_mg && soi[i] > worst { worst = soi[i]; }
    }
    if !worst.is_finite() {
        return skipped(9, title, "no IQ ≥ 30 mg samples");
    }
    if worst <= CAPS.soi_max_btdc {
        pass(9, title, format!("max SOI at IQ ≥ 30 mg = {worst:.1}° BTDC"))
    } else {
        fail(9, title, format!("max SOI = {worst:.1}° BTDC (> {} cap)", CAPS.soi_max_btdc),
            "SOI: cap warm-map cells at 26° BTDC absolute.")
    }
}

fn check10_cruise_nvh_subjective() -> CheckOutcome {
    // Spec-mandated manual check: cruise NVH cannot be inferred from a
    // log alone. v4 acceptance criterion #12 requires this to be Skipped.
    skipped_with_note(
        10,
        "Cruise NVH (subjective)",
        "manual / driver attestation required",
        "Driver-marker required, not captured by analyser.",
    )
}

fn check11_pre_post_idle_maf_delta(_log: &VcdsLog) -> CheckOutcome {
    let title = "Pre-delete vs post-delete idle MAF delta ≥ +50 mg";
    skipped_with_note(
        11, title,
        "no pre-delete log supplied",
        "Pass `--pre <PRE.csv> --post <POST.csv>` to validate-egr-delete for the full check.",
    )
}

fn check12_pre_egr_was_active(_log: &VcdsLog) -> CheckOutcome {
    let title = "Pre-delete EGR duty > 5 % observed (sanity guard)";
    skipped_with_note(
        12, title,
        "no pre-delete log supplied",
        "Pass `--pre <PRE.csv>` to validate-egr-delete for the full check.",
    )
}

fn check13_pulls_were_warm(log: &VcdsLog) -> CheckOutcome {
    let title = "Coolant reached ≥ 80 °C during pulls";
    let Some(c) = log.data.get("coolant_c") else {
        return skipped(13, title, "coolant_c channel missing");
    };
    let Some(max) = finite_max(c) else {
        return skipped(13, title, "no finite coolant samples");
    };
    if max >= CAPS.coolant_pull_min_c {
        pass(13, title, format!("max coolant = {max:.1} °C"))
    } else {
        fail(13, title, format!("max coolant = {max:.1} °C (< {} pull integrity threshold)",
                CAPS.coolant_pull_min_c),
            "Re-log on a fully warm engine before validating.")
    }
}

fn check14_fuel_temp(log: &VcdsLog) -> CheckOutcome {
    let title = "Fuel temp ≤ 80 °C";
    let Some(f) = log.data.get("fuel_temp_c") else {
        return skipped(14, title, "fuel_temp_c channel missing (firmware-dependent)");
    };
    let Some(max) = finite_max(f) else {
        return skipped(14, title, "no finite fuel_temp_c samples");
    };
    if max <= 80.0 {
        pass(14, title, format!("max fuel temp = {max:.1} °C"))
    } else {
        fail(14, title, format!("max fuel temp = {max:.1} °C (> 80 °C)"),
            "High fuel temp distorts IQ — investigate return restriction or hot soak.")
    }
}

fn check15_smooth_running(log: &VcdsLog) -> CheckOutcome {
    let title = "Smooth-running deviations ≤ ±2 mg";
    let (Some(c1), Some(c2), Some(c3)) = (
        log.data.get("srcv_cyl1"),
        log.data.get("srcv_cyl2"),
        log.data.get("srcv_cyl3"),
    ) else {
        return skipped(15, title, "smooth-running channels missing");
    };
    let n = c1.len().min(c2.len()).min(c3.len());
    let mut worst: f64 = 0.0;
    for i in 0..n {
        let cells = [c1[i], c2[i], c3[i]];
        if cells.iter().any(|x| !x.is_finite()) { continue; }
        let mean = (cells[0] + cells[1] + cells[2]) / 3.0;
        for c in cells {
            let dev = (c - mean).abs();
            if dev > worst { worst = dev; }
        }
    }
    if worst <= 2.0 {
        pass(15, title, format!("max cylinder deviation = {worst:.2} mg/stroke"))
    } else {
        fail(15, title, format!("max cylinder deviation = {worst:.2} mg/stroke (> 2.0)"),
            "Investigate injectors / cam lobe before tuning further.")
    }
}

/// Run all 15 §10 checks against `log` and return the aggregate report.
pub fn validate_egr_delete(log: &VcdsLog) -> ValidationReport {
    let items = vec![
        check1_idle_maf(log),
        check2_cruise_maf(log),
        check3_egr_duty_zero(log),
        check4_no_group_a_dtcs(log),
        check5_idle_stability(log),
        check6_lambda_floor(log),
        check7_peak_iq(log),
        check8_boost_envelope(log),
        check9_soi_envelope(log),
        check10_cruise_nvh_subjective(),
        check11_pre_post_idle_maf_delta(log),
        check12_pre_egr_was_active(log),
        check13_pulls_were_warm(log),
        check14_fuel_temp(log),
        check15_smooth_running(log),
    ];
    debug_assert_eq!(items.len(), 15);
    ValidationReport { items }
}

/// Two-log variant: also fills in the pre/post cross-checks (items 11/12).
pub fn validate_egr_delete_pre_post(pre: &VcdsLog, post: &VcdsLog) -> ValidationReport {
    let mut report = validate_egr_delete(post);

    // Item 11: pre-delete vs post-delete idle MAF delta ≥ +50 mg.
    let title11 = "Pre-delete vs post-delete idle MAF delta ≥ +50 mg";
    let pre_idle = idle_min_maf(pre);
    let post_idle = idle_min_maf(post);
    report.items[10] = match (pre_idle, post_idle) {
        (Some(p), Some(q)) => {
            let delta = q - p;
            if delta >= 50.0 {
                pass(11, title11, format!("idle MAF Δ = +{delta:.0} mg/stroke (pre {p:.0} → post {q:.0})"))
            } else {
                fail(11, title11, format!("idle MAF Δ = +{delta:.1} mg/stroke (< 50)"),
                    "Delete may not be flashed in pre-delete log; verify the pre-delete log captured EGR active.")
            }
        }
        _ => skipped(11, title11, "could not extract idle MAF from one or both logs"),
    };

    // Item 12: pre-delete EGR duty > 5 % observed (proves the test was real).
    let title12 = "Pre-delete EGR duty > 5 % observed (sanity guard)";
    let pre_max_duty = pre.data.get("egr_duty").and_then(|d| finite_max(d));
    report.items[11] = match pre_max_duty {
        Some(m) if m > 5.0 => pass(12, title12, format!("pre-delete max EGR duty = {m:.1} %")),
        Some(m) => fail(12, title12, format!("pre-delete max EGR duty = {m:.1} % (≤ 5)"),
            "Pre-delete log shows no EGR activity — was the delete already flashed when the pre-log was captured?"),
        None => skipped(12, title12, "pre-delete log has no egr_duty channel"),
    };

    report
}

fn idle_min_maf(log: &VcdsLog) -> Option<f64> {
    let coolant = log.data.get("coolant_c")?;
    let maf = log.data.get("maf_actual")?;
    let pedal = log.data.get("pedal_pct");
    let iq = log.data.get("iq_requested").or_else(|| log.data.get("iq_actual"));
    let mut samples: Vec<f64> = Vec::new();
    for i in 0..maf.len() {
        if i >= coolant.len() || !coolant[i].is_finite() || coolant[i] < CAPS.warm_coolant_min_c {
            continue;
        }
        let idle_ok = match pedal {
            Some(p) if i < p.len() && p[i].is_finite() => p[i] <= IDLE_PEDAL_MAX_PCT,
            _ => match iq {
                Some(q) if i < q.len() && q[i].is_finite() => q[i] <= IDLE_IQ_MAX_MG,
                _ => true,
            },
        };
        if !idle_ok || !maf[i].is_finite() { continue; }
        samples.push(maf[i]);
    }
    finite_min(&samples)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ingest::VcdsLog;
    use std::collections::{BTreeMap, BTreeSet};
    use std::path::PathBuf;

    fn synth(channels: &[(&str, Vec<f64>)], dtcs: Vec<String>) -> VcdsLog {
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
            dtcs,
        }
    }

    fn empty_log() -> VcdsLog {
        synth(&[], Vec::new())
    }

    #[test]
    fn check10_is_always_skipped_with_driver_marker_note() {
        // v4 acceptance #12: check 10 is always Skipped with the
        // driver-marker note.
        let report = validate_egr_delete(&empty_log());
        let item10 = &report.items[9];
        assert_eq!(item10.id, 10);
        assert_eq!(item10.status, CheckStatus::Skipped);
        assert!(item10.remediation.contains("Driver-marker required"));
    }

    #[test]
    fn empty_log_skips_most_checks() {
        let report = validate_egr_delete(&empty_log());
        assert_eq!(report.items.len(), 15);
        assert!(report.skipped() >= 12);
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
            ("pedal_pct",    vec![3.0; n]),
            ("soi_actual",   vec![10.0; n]),
        ], Vec::new());
        let r = validate_egr_delete(&log);
        let failed: Vec<&str> = r.items.iter()
            .filter(|i| i.status == CheckStatus::Fail)
            .map(|i| i.title.as_str()).collect();
        assert!(r.pass(), "post-delete happy path must pass: failed = {failed:?}");
    }

    #[test]
    fn pre_delete_log_fails_egr_duty_check() {
        let n = 60;
        let log = synth(&[
            ("rpm",          vec![820.0; n]),
            ("egr_duty",     vec![35.0; n]), // EGR active
            ("coolant_c",    vec![88.0; n]),
            ("iq_requested", vec![4.0; n]),
            ("iq_actual",    vec![4.0; n]),
            ("maf_actual",   vec![220.0; n]),
            ("maf_spec",     vec![220.0; n]),
            ("boost_actual", vec![1000.0; n]),
            ("pedal_pct",    vec![3.0; n]),
        ], Vec::new());
        let r = validate_egr_delete(&log);
        assert!(!r.pass(), "pre-delete log must fail validation");
        let item3 = r.items.iter().find(|i| i.id == 3).unwrap();
        assert_eq!(item3.status, CheckStatus::Fail);
    }

    #[test]
    fn p0403_in_dtc_sidecar_fails_check4() {
        let n = 30;
        let log = synth(&[
            ("rpm",          vec![820.0; n]),
            ("egr_duty",     vec![0.0; n]),
            ("coolant_c",    vec![88.0; n]),
            ("iq_actual",    vec![4.0; n]),
            ("maf_actual",   vec![300.0; n]),
            ("maf_spec",     vec![850.0; n]),
            ("boost_actual", vec![1000.0; n]),
            ("pedal_pct",    vec![3.0; n]),
        ], vec!["P0403".to_string()]);
        let r = validate_egr_delete(&log);
        let item4 = r.items.iter().find(|i| i.id == 4).unwrap();
        assert_eq!(item4.status, CheckStatus::Fail);
    }

    #[test]
    fn idle_instability_fails_check5() {
        let n = 200;
        let mut rpm = Vec::with_capacity(n);
        for i in 0..n {
            rpm.push(820.0 + (i as f64).sin() * 80.0); // σ ≫ 25
        }
        let log = synth(&[
            ("rpm",        rpm),
            ("egr_duty",   vec![0.0; n]),
            ("coolant_c",  vec![88.0; n]),
            ("pedal_pct",  vec![3.0; n]),
            ("iq_actual",  vec![4.0; n]),
            ("maf_actual", vec![300.0; n]),
            ("maf_spec",   vec![850.0; n]),
            ("boost_actual", vec![1000.0; n]),
        ], Vec::new());
        let r = validate_egr_delete(&log);
        let item5 = r.items.iter().find(|i| i.id == 5).unwrap();
        assert_eq!(item5.status, CheckStatus::Fail);
    }

    #[test]
    fn report_markdown_renders_glyphs() {
        let report = validate_egr_delete(&empty_log());
        let md = report.to_markdown();
        assert!(md.contains("## EGR Delete Validation Checklist"));
        assert!(md.contains("[-]"));
        assert!(md.contains("**Result:"));
        assert!(md.contains("Driver-marker required"));
    }

    #[test]
    fn pre_post_helper_fills_items_11_and_12() {
        let n = 30;
        let pre = synth(&[
            ("rpm",          vec![820.0; n]),
            ("egr_duty",     vec![32.0; n]),
            ("coolant_c",    vec![88.0; n]),
            ("iq_actual",    vec![4.0; n]),
            ("maf_actual",   vec![205.0; n]),
            ("pedal_pct",    vec![3.0; n]),
        ], Vec::new());
        let post = synth(&[
            ("rpm",          vec![820.0; n]),
            ("egr_duty",     vec![0.0; n]),
            ("coolant_c",    vec![88.0; n]),
            ("iq_actual",    vec![4.0; n]),
            ("iq_requested", vec![4.0; n]),
            ("maf_actual",   vec![300.0; n]),
            ("maf_spec",     vec![850.0; n]),
            ("boost_actual", vec![1000.0; n]),
            ("pedal_pct",    vec![3.0; n]),
        ], Vec::new());
        let r = validate_egr_delete_pre_post(&pre, &post);
        assert_eq!(r.items[10].status, CheckStatus::Pass, "item 11 should pass: pre=205 → post=300 Δ=95 ≥ 50");
        assert_eq!(r.items[11].status, CheckStatus::Pass, "item 12 should pass: pre EGR duty 32 % > 5 %");
    }
}
