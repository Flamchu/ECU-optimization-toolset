//! Rule pack for AMF / EDC15P+ — R01..R15.
//!
//! Each rule below carries its rationale as a docstring (per spec §10
//! implementation note) so it surfaces in `--help` and the Markdown
//! report.

use crate::platform::amf_edc15p::egr::{
    in_cruise_band, CRUISE_PEDAL_MAX_PCT, DTC_LIST_TO_SUPPRESS, DTC_WIRING_FAULTS,
    EGR_DUTY_OBSERVED_TOLERANCE_PCT, MAF_DEVIATION_FRACTION,
    MAF_DEVIATION_MIN_DURATION_S, MAF_EXCESS_INFO_MG, COLD_START_COOLANT_CUTOFF_C,
    WARM_COOLANT_MIN_C, WOT_PEDAL_CUTOFF_PCT,
};
use crate::platform::amf_edc15p::envelope::{CAPS, DIESEL_AFR_STOICH, NM_PER_MG_IQ};
use crate::platform::amf_edc15p::stock_refs::stock_boost_at_rpm;
use crate::rules::base::{make_skipped, Finding, Rule, Severity};
use crate::util::timebase::ResampledLog;
use crate::util::Pull;

// ---------------------------------------------------------------------------
// Rule definitions
// ---------------------------------------------------------------------------

/// R01 — Underboost.
pub const R01: Rule = Rule {
    id: "R01",
    severity: Severity::Warn,
    rationale_one_liner:
        "KP35 PID can't keep up: leak, sticky wastegate, or LDRXN ramp too steep for turbo.",
    recommended_delta_ref: Some("LDRXN/N75_duty"),
    requires_channels: &["rpm", "boost_actual", "boost_spec"],
    requires_groups: &["011"],
};

/// R02 — Overboost spike.
pub const R02: Rule = Rule {
    id: "R02",
    severity: Severity::Critical,
    rationale_one_liner:
        "KP35 sustained over 2150 mbar pushes shaft past the right edge of the compressor map → over-speed.",
    recommended_delta_ref: Some("LDRXN: lower target"),
    requires_channels: &["boost_actual", "boost_spec"],
    requires_groups: &["011"],
};

/// R03 — Boost target excessive.
pub const R03: Rule = Rule {
    id: "R03",
    severity: Severity::Critical,
    rationale_one_liner: "Hard envelope ceiling for KP35 longevity.",
    recommended_delta_ref: Some("LDRXN: rpm 2000-3500"),
    requires_channels: &["rpm", "boost_spec"],
    requires_groups: &["011"],
};

/// R04 — High-RPM boost not tapering.
pub const R04: Rule = Rule {
    id: "R04",
    severity: Severity::Warn,
    rationale_one_liner:
        "KP35 is choke-flow-limited: you must back off above 4000 to keep it in the efficiency island.",
    recommended_delta_ref: Some("LDRXN taper rpm 4000-4500"),
    requires_channels: &["rpm", "boost_spec"],
    requires_groups: &["011"],
};

/// R05 — MAF below spec.
pub const R05: Rule = Rule {
    id: "R05",
    severity: Severity::Warn,
    rationale_one_liner:
        "MAF drift, dirty intake, boost leak, or MAF aging — fueling decisions become wrong.",
    recommended_delta_ref: Some("MLHFM (only if MAF replaced)"),
    requires_channels: &["maf_actual", "maf_spec"],
    requires_groups: &["003"],
};

/// R06 — Lambda floor breach (v3 floor: 1.05).
pub const R06: Rule = Rule {
    id: "R06",
    severity: Severity::Critical,
    rationale_one_liner:
        "Below λ = 1.05 = past stoich → incomplete combustion → EGT spike → ring-land cracks. v3 controlled-environment context relaxes the v2 1.20 floor but keeps 1.05 as a physical limit.",
    recommended_delta_ref: Some("Smoke_IQ_by_MAF + Smoke_IQ_by_MAP: enforce λ ≥ 1.05"),
    requires_channels: &["maf_actual", "iq_requested"],
    requires_groups: &["003", "008"],
};

/// R07 — Peak IQ above sane envelope (v3 cap: 54 mg/stroke).
pub const R07: Rule = Rule {
    id: "R07",
    severity: Severity::Critical,
    rationale_one_liner:
        "Above 54 mg/stroke the stock LUK clutch and stock injectors run out of headroom. v3 raised from 52 in v2 — smoke is acceptable in the controlled-env context, but injector duty and clutch torque are still hard limits.",
    recommended_delta_ref: Some("Driver_Wish + Duration"),
    requires_channels: &["iq_requested"],
    requires_groups: &["008"],
};

/// R08 — Modelled torque above clutch ceiling.
pub const R08: Rule = Rule {
    id: "R08",
    severity: Severity::Critical,
    rationale_one_liner:
        "LUK SMF rated ~195 Nm + ~20 % = 240 Nm hard ceiling. Above this the clutch slips within weeks.",
    recommended_delta_ref: Some("Torque_Limiter: clamp peak ≤ 240 Nm"),
    requires_channels: &["iq_requested"],
    requires_groups: &["008"],
};

/// R09 — SOI excess.
pub const R09: Rule = Rule {
    id: "R09",
    severity: Severity::Critical,
    rationale_one_liner:
        "Beyond 26° BTDC peak cylinder pressure migrates ahead of TDC → piston crown stress; cam-lobe physical limit is ~35° but safe usable limit is 26-28°.",
    recommended_delta_ref: Some("SOI: cap at 26° BTDC absolute"),
    requires_channels: &["soi_actual", "iq_requested"],
    requires_groups: &["020", "008"],
};

/// R10 — EOI late.
pub const R10: Rule = Rule {
    id: "R10",
    severity: Severity::Warn,
    rationale_one_liner:
        "Combustion past ~6-10° ATDC dumps unburned heat into the turbine → high EGT, poor BSFC.",
    recommended_delta_ref: Some("SOI/Duration: re-balance"),
    requires_channels: &["soi_actual", "iq_requested", "rpm"],
    requires_groups: &["020", "008"],
};

/// R11 — Coolant too low for pull.
pub const R11: Rule = Rule {
    id: "R11",
    severity: Severity::Info,
    rationale_one_liner:
        "EDC15P+ uses cold SOI map below 80 °C — your data isn't representative of warm calibration. Re-do the pull.",
    recommended_delta_ref: None,
    requires_channels: &["coolant_c"],
    requires_groups: &["001"],
};

/// R12 — Atmospheric correction missing.
pub const R12: Rule = Rule {
    id: "R12",
    severity: Severity::Info,
    rationale_one_liner:
        "Without ambient pressure capture (key-on, engine-off, group 010), altitude derate can't be assessed.",
    recommended_delta_ref: None,
    requires_channels: &["atm_pressure"],
    requires_groups: &["010"],
};

/// R13 — Fuel temp high.
pub const R13: Rule = Rule {
    id: "R13",
    severity: Severity::Warn,
    rationale_one_liner:
        "High fuel temp = lower density = lower delivered IQ for same duration → boost target overshoots fuelling.",
    recommended_delta_ref: None,
    requires_channels: &["fuel_temp_c"],
    requires_groups: &["013"],
};

/// R14 — Smooth-running deviation.
pub const R14: Rule = Rule {
    id: "R14",
    severity: Severity::Warn,
    rationale_one_liner:
        "Indicates worn injector cam lobe (PD weak point) or failing injector. Tuning a sick engine = killing it faster.",
    recommended_delta_ref: None,
    requires_channels: &["srcv_cyl1", "srcv_cyl2", "srcv_cyl3"],
    requires_groups: &["013"],
};

/// R15 — Limp / DTC interruption.
pub const R15: Rule = Rule {
    id: "R15",
    severity: Severity::Warn,
    rationale_one_liner: "ECU is in limp mode — log is not valid for tuning.",
    recommended_delta_ref: None,
    requires_channels: &["n75_duty"],
    requires_groups: &["011"],
};

// ---------------------------------------------------------------------------
// v3 EGR-delete rule additions
// ---------------------------------------------------------------------------

/// R16 — EGR duty observed (delete not applied).
pub const R16: Rule = Rule {
    id: "R16",
    severity: Severity::Critical,
    rationale_one_liner:
        "v3 mandates a software EGR delete. Any EGR duty > 2 % at any sample means the delete was not flashed, was applied to only one bank, or was overridden by the spec-MAF map polarity. Re-flash both banks of arwMEAB0KL → 0 % AND arwMLGRDKF → ≥850 mg/stroke.",
    recommended_delta_ref: Some("AGR_arwMEAB0KL + arwMLGRDKF"),
    requires_channels: &["egr_duty"],
    requires_groups: &["003"],
};

/// R17 — MAF deviation post-delete (cruise, warm).
pub const R17: Rule = Rule {
    id: "R17",
    severity: Severity::Warn,
    rationale_one_liner:
        "Post-delete cruise MAF should track spec-MAF closely (Strategy A) or sit cleanly above it (Strategy B). >15 % deviation sustained for >2 s indicates the spec-MAF map was not rescaled correctly.",
    recommended_delta_ref: Some("arwMLGRDKF: re-flatten ≥850 mg/stroke"),
    requires_channels: &["maf_actual", "maf_spec"],
    requires_groups: &["003"],
};

/// R17b — MAF excess at cruise with EGR=0 (info, "delete is functional").
pub const R17B: Rule = Rule {
    id: "R17b",
    severity: Severity::Info,
    rationale_one_liner:
        "Post-delete with Strategy-B saturation, MAF actual will sit well below spec-MAF (which is intentionally 850). MAF actual exceeding spec by ≥50 mg with EGR=0 confirms the delete is functional and the saturation is harmless.",
    recommended_delta_ref: None,
    requires_channels: &["maf_actual", "maf_spec", "egr_duty"],
    requires_groups: &["003"],
};

/// R18 — Cruise SOI NVH check.
pub const R18: Rule = Rule {
    id: "R18",
    severity: Severity::Info,
    rationale_one_liner:
        "Cruise-band SOI is unchanged from stock with EGR off. The premixed phase is faster without inert charge, which can raise NVH. Apply the −1.0° SOI retard to warm SOI maps 0..3 in the cruise band only if subjective NVH is objectionable.",
    recommended_delta_ref: Some("SOI_warm_cruise"),
    requires_channels: &["soi_actual", "egr_duty", "rpm", "iq_requested"],
    requires_groups: &["020", "003", "008"],
};

/// R19 — DTC scan check.
pub const R19: Rule = Rule {
    id: "R19",
    severity: Severity::Warn,
    rationale_one_liner:
        "P0401 / P0402 in a post-flash log mean the DTC was not suppressed. P0403 is a real solenoid wiring fault — investigate, do NOT just suppress. P0404 / P0405 / P0406 are unusual on AMF (no EGR position sensor) — flag for investigation.",
    recommended_delta_ref: Some("DTC_thresholds"),
    requires_channels: &["dtc_codes"],
    requires_groups: &[],
};

/// All rules in canonical order (v3: R01..R15 + R16, R17, R17b, R18, R19).
pub const ALL_RULES: &[&Rule] = &[
    &R01, &R02, &R03, &R04, &R05, &R06, &R07,
    &R08, &R09, &R10, &R11, &R12, &R13, &R14, &R15,
    &R16, &R17, &R17B, &R18, &R19,
];

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Return slice indices `[i_start, i_end)` for the given pull.
fn slice<'a>(log: &'a ResampledLog, pull: &Pull, name: &str) -> Option<&'a [f64]> {
    let v = log.get(name)?;
    if pull.i_end > v.len() || pull.i_start > pull.i_end {
        return None;
    }
    Some(&v[pull.i_start..pull.i_end])
}

fn slice_time<'a>(log: &'a ResampledLog, pull: &Pull) -> &'a [f64] {
    let len = log.time.len();
    let end = pull.i_end.min(len);
    let start = pull.i_start.min(end);
    &log.time[start..end]
}

fn has_all(log: &ResampledLog, names: &[&str]) -> bool {
    names.iter().all(|n| log.has(n))
}

fn finite_max(xs: &[f64]) -> Option<f64> {
    xs.iter().cloned().filter(|x| x.is_finite())
        .fold(None, |acc, x| match acc {
            Some(a) if a >= x => Some(a),
            _ => Some(x),
        })
}

fn finite_min(xs: &[f64]) -> Option<f64> {
    xs.iter().cloned().filter(|x| x.is_finite())
        .fold(None, |acc, x| match acc {
            Some(a) if a <= x => Some(a),
            _ => Some(x),
        })
}

fn finite_mean(xs: &[f64]) -> Option<f64> {
    let mut sum = 0.0;
    let mut n = 0usize;
    for &x in xs { if x.is_finite() { sum += x; n += 1; } }
    if n == 0 { None } else { Some(sum / n as f64) }
}

fn median_dt(times: &[f64]) -> f64 {
    if times.len() < 2 { return 0.2; }
    let mut diffs: Vec<f64> = times.windows(2)
        .map(|w| w[1] - w[0])
        .filter(|d| d.is_finite() && *d > 0.0)
        .collect();
    if diffs.is_empty() { return 0.2; }
    diffs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    diffs[diffs.len() / 2]
}

/// PD injection duration model: ~0.55°/mg at low RPM, scales with RPM.
fn model_duration_deg(iq_mg: f64, rpm: f64) -> f64 {
    let scale = (rpm.max(600.0) / 3000.0).sqrt();
    iq_mg * 0.55 * scale
}

fn one(
    rule: &Rule,
    pull: &Pull,
    severity: Severity,
    observed: f64,
    threshold: f64,
    rationale: &str,
    action: Option<&str>,
) -> Finding {
    Finding {
        rule_id: rule.id,
        severity,
        pull_id: pull.pull_id,
        t_start: pull.t_start,
        t_end: pull.t_end,
        observed_extreme: observed,
        threshold,
        rationale: rationale.to_string(),
        recommended_action_ref: action.map(str::to_string),
        skipped: false,
    }
}

// ---------------------------------------------------------------------------
// Rule predicates
// ---------------------------------------------------------------------------

/// R01 — Underboost: actual < spec − 150 mbar for ≥ 1.0 s above 2000 rpm.
pub fn r01_underboost(log: &ResampledLog, pull: &Pull) -> Vec<Finding> {
    if !has_all(log, &["rpm", "boost_actual", "boost_spec"]) {
        return vec![make_skipped(&R01, pull, "channels rpm/boost_spec/boost_actual missing")];
    }
    let rpm = match slice(log, pull, "rpm") { Some(v) => v, None => return vec![] };
    let actual = match slice(log, pull, "boost_actual") { Some(v) => v, None => return vec![] };
    let spec = match slice(log, pull, "boost_spec") { Some(v) => v, None => return vec![] };
    let times = slice_time(log, pull);

    let n = rpm.len();
    if n == 0 { return vec![]; }
    let mut breach = vec![false; n];
    for i in 0..n {
        if !rpm[i].is_finite() || !actual[i].is_finite() || !spec[i].is_finite() {
            continue;
        }
        let err = spec[i] - actual[i];
        breach[i] = rpm[i] >= 2000.0 && err >= 150.0;
    }
    if !breach.iter().any(|&b| b) {
        return vec![];
    }
    let dt = median_dt(times);
    let min_run = ((1.0 / dt).round() as usize).max(1);
    let mut found_run = false;
    let mut i = 0;
    while i < n {
        if breach[i] {
            let mut j = i;
            while j < n && breach[j] { j += 1; }
            if (j - i) >= min_run {
                found_run = true;
                break;
            }
            i = j;
        } else {
            i += 1;
        }
    }
    if !found_run { return vec![]; }
    let mut worst: f64 = 0.0;
    for i in 0..n {
        if !spec[i].is_finite() || !actual[i].is_finite() { continue; }
        worst = worst.max(spec[i] - actual[i]);
    }
    vec![one(
        &R01, pull, Severity::Warn, worst, 150.0,
        R01.rationale_one_liner,
        Some("LDRXN: re-check N75 PID; only adjust if R01 fires repeatedly."),
    )]
}

/// R02 — Overboost: actual > spec + 200 mbar OR > 2200 mbar abs.
pub fn r02_overboost_spike(log: &ResampledLog, pull: &Pull) -> Vec<Finding> {
    if !has_all(log, &["boost_actual", "boost_spec"]) {
        return vec![make_skipped(&R02, pull, "channels boost_actual/boost_spec missing")];
    }
    let actual = match slice(log, pull, "boost_actual") { Some(v) => v, None => return vec![] };
    let spec = match slice(log, pull, "boost_spec") { Some(v) => v, None => return vec![] };
    let n = actual.len();
    let mut spike = false;
    for i in 0..n {
        if !actual[i].is_finite() || !spec[i].is_finite() { continue; }
        if actual[i] > spec[i] + 200.0 || actual[i] > 2200.0 {
            spike = true;
            break;
        }
    }
    if !spike { return vec![]; }
    let observed = finite_max(actual).unwrap_or(0.0);
    vec![one(
        &R02, pull, Severity::Critical, observed, 2200.0,
        R02.rationale_one_liner,
        Some("LDRXN: rpm 2000-3500 × IQ 40-50 mg → lower target"),
    )]
}

/// R03 — Boost target excessive: any spec > 2150 mbar abs, or > stock+250.
pub fn r03_boost_target_excessive(log: &ResampledLog, pull: &Pull) -> Vec<Finding> {
    if !has_all(log, &["rpm", "boost_spec"]) {
        return vec![make_skipped(&R03, pull, "channels rpm/boost_spec missing")];
    }
    let rpm = match slice(log, pull, "rpm") { Some(v) => v, None => return vec![] };
    let spec = match slice(log, pull, "boost_spec") { Some(v) => v, None => return vec![] };
    let n = spec.len();
    let mut breach = false;
    for i in 0..n {
        if !rpm[i].is_finite() || !spec[i].is_finite() { continue; }
        if spec[i] > f64::from(CAPS.peak_boost_mbar_abs) {
            breach = true;
            break;
        }
        if spec[i] > stock_boost_at_rpm(rpm[i]) + 250.0 {
            breach = true;
            break;
        }
    }
    if !breach { return vec![]; }
    vec![one(
        &R03, pull, Severity::Critical,
        finite_max(spec).unwrap_or(0.0),
        f64::from(CAPS.peak_boost_mbar_abs),
        R03.rationale_one_liner,
        Some("LDRXN: cap any cell ≤ 2150 mbar absolute."),
    )]
}

/// R04 — boost_spec @ 4500 > boost_spec @ 3000 − 100 mbar.
pub fn r04_no_taper(log: &ResampledLog, pull: &Pull) -> Vec<Finding> {
    if !has_all(log, &["rpm", "boost_spec"]) {
        return vec![make_skipped(&R04, pull, "channels rpm/boost_spec missing")];
    }
    let rpm = match slice(log, pull, "rpm") { Some(v) => v, None => return vec![] };
    let spec = match slice(log, pull, "boost_spec") { Some(v) => v, None => return vec![] };
    let near_3000: Vec<f64> = rpm.iter().zip(spec.iter())
        .filter(|(r, s)| (2900.0..=3100.0).contains(*r) && s.is_finite())
        .map(|(_, s)| *s).collect();
    let near_4500: Vec<f64> = rpm.iter().zip(spec.iter())
        .filter(|(r, s)| (4400.0..=4600.0).contains(*r) && s.is_finite())
        .map(|(_, s)| *s).collect();
    if near_3000.is_empty() || near_4500.is_empty() {
        return vec![];
    }
    let med = |xs: &[f64]| -> f64 {
        let mut v: Vec<f64> = xs.to_vec();
        v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        v[v.len() / 2]
    };
    let s3000 = med(&near_3000);
    let s4500 = med(&near_4500);
    if s4500 <= s3000 - 100.0 { return vec![]; }
    vec![one(
        &R04, pull, Severity::Warn, s4500, s3000 - 100.0,
        R04.rationale_one_liner,
        Some("LDRXN taper: hold rpm 4000-4500 at stock − 50 mbar"),
    )]
}

/// R05 — maf_actual < maf_spec − 8 % across the pull.
pub fn r05_maf_below_spec(log: &ResampledLog, pull: &Pull) -> Vec<Finding> {
    if !has_all(log, &["maf_actual", "maf_spec"]) {
        return vec![make_skipped(&R05, pull, "channels maf_actual/maf_spec missing")];
    }
    let actual = match slice(log, pull, "maf_actual") { Some(v) => v, None => return vec![] };
    let spec = match slice(log, pull, "maf_spec") { Some(v) => v, None => return vec![] };
    let spec_mean = match finite_mean(spec) { Some(v) if v > 0.0 => v, _ => return vec![] };
    let actual_mean = match finite_mean(actual) { Some(v) => v, None => return vec![] };
    let deficit = (spec_mean - actual_mean) / spec_mean;
    if deficit < 0.08 { return vec![]; }
    vec![one(
        &R05, pull, Severity::Warn, actual_mean, spec_mean * 0.92,
        R05.rationale_one_liner,
        Some("Inspect intake/MAF before tuning."),
    )]
}

/// R06 — Lambda floor breach: any sample where λ < 1.20.
pub fn r06_lambda_floor(log: &ResampledLog, pull: &Pull) -> Vec<Finding> {
    if !has_all(log, &["maf_actual", "iq_requested"]) {
        return vec![make_skipped(&R06, pull, "channels maf_actual/iq_requested missing")];
    }
    let maf = match slice(log, pull, "maf_actual") { Some(v) => v, None => return vec![] };
    let iq = match slice(log, pull, "iq_requested") { Some(v) => v, None => return vec![] };
    let n = maf.len();
    let mut min_lambda = f64::INFINITY;
    for i in 0..n {
        if !maf[i].is_finite() || !iq[i].is_finite() || iq[i] <= 0.0 {
            continue;
        }
        let lam = maf[i] / (iq[i] * DIESEL_AFR_STOICH);
        if lam < min_lambda { min_lambda = lam; }
    }
    if !min_lambda.is_finite() || min_lambda >= CAPS.lambda_floor { return vec![]; }
    vec![one(
        &R06, pull, Severity::Critical, min_lambda, CAPS.lambda_floor,
        R06.rationale_one_liner,
        Some("Smoke_IQ_by_MAF/MAP: raise IQ cap so λ ≥ 1.20 at this MAF."),
    )]
}

/// R07 — IQ requested > 52 mg/stroke.
pub fn r07_peak_iq(log: &ResampledLog, pull: &Pull) -> Vec<Finding> {
    if !log.has("iq_requested") {
        return vec![make_skipped(&R07, pull, "channel iq_requested missing")];
    }
    let iq = match slice(log, pull, "iq_requested") { Some(v) => v, None => return vec![] };
    let peak = finite_max(iq).unwrap_or(0.0);
    if peak <= CAPS.peak_iq_mg { return vec![]; }
    vec![one(
        &R07, pull, Severity::Critical, peak, CAPS.peak_iq_mg,
        R07.rationale_one_liner,
        Some("Driver_Wish: cap WOT request at 52 mg/stroke."),
    )]
}

/// R08 — Modelled flywheel torque (4.4 Nm/mg) > 240 Nm.
pub fn r08_torque_ceiling(log: &ResampledLog, pull: &Pull) -> Vec<Finding> {
    if !log.has("iq_requested") {
        return vec![make_skipped(&R08, pull, "channel iq_requested missing")];
    }
    let iq = match slice(log, pull, "iq_requested") { Some(v) => v, None => return vec![] };
    let peak_iq = finite_max(iq).unwrap_or(0.0);
    let modelled_nm = peak_iq * NM_PER_MG_IQ;
    if modelled_nm <= CAPS.modelled_flywheel_torque_nm { return vec![]; }
    vec![one(
        &R08, pull, Severity::Critical,
        modelled_nm, CAPS.modelled_flywheel_torque_nm,
        R08.rationale_one_liner,
        Some("Torque_Limiter: cap modelled peak at 240 Nm."),
    )]
}

/// R09 — soi_actual > 26° BTDC at any IQ ≥ 30 mg.
///
/// On a `LOW_RATE` pull, severity is downgraded to warn because SOI
/// transients can be missed at the slow VCDS sample rate.
pub fn r09_soi_excess(log: &ResampledLog, pull: &Pull, low_rate: bool) -> Vec<Finding> {
    if !has_all(log, &["soi_actual", "iq_requested"]) {
        return vec![make_skipped(&R09, pull, "channels soi_actual/iq_requested missing")];
    }
    let soi = match slice(log, pull, "soi_actual") { Some(v) => v, None => return vec![] };
    let iq = match slice(log, pull, "iq_requested") { Some(v) => v, None => return vec![] };
    let n = soi.len();
    let mut worst = f64::NEG_INFINITY;
    for i in 0..n {
        if !soi[i].is_finite() || !iq[i].is_finite() { continue; }
        if iq[i] >= CAPS.soi_iq_threshold_mg && soi[i] > CAPS.soi_max_btdc && soi[i] > worst {
            worst = soi[i];
        }
    }
    if !worst.is_finite() { return vec![]; }
    let sev = if low_rate { Severity::Warn } else { Severity::Critical };
    vec![one(
        &R09, pull, sev, worst, CAPS.soi_max_btdc,
        R09.rationale_one_liner,
        Some("SOI: cap warm-map cells at 26° BTDC absolute."),
    )]
}

/// R10 — Computed EOI = SOI − duration_model > 10° ATDC.
pub fn r10_eoi_late(log: &ResampledLog, pull: &Pull, _low_rate: bool) -> Vec<Finding> {
    if !has_all(log, &["soi_actual", "iq_requested", "rpm"]) {
        return vec![make_skipped(&R10, pull, "channels soi_actual/iq_requested/rpm missing")];
    }
    let soi = match slice(log, pull, "soi_actual") { Some(v) => v, None => return vec![] };
    let iq = match slice(log, pull, "iq_requested") { Some(v) => v, None => return vec![] };
    let rpm = match slice(log, pull, "rpm") { Some(v) => v, None => return vec![] };
    let n = soi.len();
    let mut worst = f64::NEG_INFINITY;
    for i in 0..n {
        if !soi[i].is_finite() || !iq[i].is_finite() || !rpm[i].is_finite() { continue; }
        let dur = model_duration_deg(iq[i], rpm[i]);
        let eoi_atdc = -(soi[i] - dur);
        if eoi_atdc > CAPS.eoi_max_atdc && eoi_atdc > worst {
            worst = eoi_atdc;
        }
    }
    if !worst.is_finite() { return vec![]; }
    vec![one(
        &R10, pull, Severity::Warn, worst, CAPS.eoi_max_atdc,
        &format!("{} (duration modelled, not measured)", R10.rationale_one_liner),
        Some("SOI/Duration: tighten duration before extending IQ further."),
    )]
}

/// R11 — Coolant < 80 °C during pull.
pub fn r11_coolant_low(log: &ResampledLog, pull: &Pull) -> Vec<Finding> {
    if !log.has("coolant_c") {
        return vec![make_skipped(&R11, pull, "channel coolant_c missing")];
    }
    let c = match slice(log, pull, "coolant_c") { Some(v) => v, None => return vec![] };
    let min = finite_min(c).unwrap_or(f64::INFINITY);
    if min >= 80.0 { return vec![]; }
    vec![one(
        &R11, pull, Severity::Info, min, 80.0,
        R11.rationale_one_liner, None,
    )]
}

/// R12 — Group 010 absent OR atm_pressure all-NaN.
pub fn r12_no_atm(log: &ResampledLog, pull: &Pull) -> Vec<Finding> {
    let present = log.data.get("atm_pressure")
        .is_some_and(|v| v.iter().any(|x| x.is_finite()));
    if present { return vec![]; }
    vec![one(
        &R12, pull, Severity::Info, 0.0, 0.0,
        R12.rationale_one_liner, None,
    )]
}

/// R13 — fuel_temp_c > 80 °C during pull.
pub fn r13_fuel_temp(log: &ResampledLog, pull: &Pull) -> Vec<Finding> {
    if !log.has("fuel_temp_c") {
        return vec![make_skipped(&R13, pull, "channel fuel_temp_c missing (firmware-dependent)")];
    }
    let f = match slice(log, pull, "fuel_temp_c") { Some(v) => v, None => return vec![] };
    let max = finite_max(f).unwrap_or(f64::NEG_INFINITY);
    if max <= 80.0 { return vec![]; }
    vec![one(
        &R13, pull, Severity::Warn, max, 80.0,
        R13.rationale_one_liner, None,
    )]
}

/// R14 — Any cylinder smooth-running > ±2.0 mg from mean.
pub fn r14_srcv(log: &ResampledLog, pull: &Pull) -> Vec<Finding> {
    if !has_all(log, &["srcv_cyl1", "srcv_cyl2", "srcv_cyl3"]) {
        return vec![make_skipped(&R14, pull, "smooth-running channels missing")];
    }
    let c1 = match slice(log, pull, "srcv_cyl1") { Some(v) => v, None => return vec![] };
    let c2 = match slice(log, pull, "srcv_cyl2") { Some(v) => v, None => return vec![] };
    let c3 = match slice(log, pull, "srcv_cyl3") { Some(v) => v, None => return vec![] };
    let n = c1.len();
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
    if worst < 2.0 { return vec![]; }
    vec![one(
        &R14, pull, Severity::Warn, worst, 2.0,
        R14.rationale_one_liner, None,
    )]
}

/// R15 — N75 duty clamped to a single value across the entire pull.
pub fn r15_limp(log: &ResampledLog, pull: &Pull) -> Vec<Finding> {
    if !log.has("n75_duty") {
        return vec![make_skipped(&R15, pull, "channel n75_duty missing")];
    }
    let n75 = match slice(log, pull, "n75_duty") { Some(v) => v, None => return vec![] };
    let finite: Vec<f64> = n75.iter().cloned().filter(|x| x.is_finite()).collect();
    if finite.is_empty() { return vec![]; }
    let max = finite.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let min = finite.iter().cloned().fold(f64::INFINITY, f64::min);
    let spread = max - min;
    if spread > 1.0 { return vec![]; }
    vec![one(
        &R15, pull, Severity::Warn, finite[0], 1.0,
        R15.rationale_one_liner, None,
    )]
}

// ---------------------------------------------------------------------------
// v3 EGR-delete rule predicates
// ---------------------------------------------------------------------------

/// R16 — Any sample with EGR duty > 2 % anywhere in the log (not just
/// the pull). Evaluated against the entire log on the first pull only,
/// so the same finding is not duplicated per pull.
pub fn r16_egr_observed(log: &ResampledLog, pull: &Pull) -> Vec<Finding> {
    if pull.pull_id != 1 {
        return vec![];
    }
    if !log.has("egr_duty") {
        return vec![make_skipped(&R16, pull, "channel egr_duty missing")];
    }
    let duty = match log.get("egr_duty") { Some(v) => v, None => return vec![] };
    let max_observed = duty.iter().copied()
        .filter(|x| x.is_finite())
        .fold(f64::NEG_INFINITY, f64::max);
    if !max_observed.is_finite() || max_observed <= EGR_DUTY_OBSERVED_TOLERANCE_PCT {
        return vec![];
    }
    vec![one(
        &R16, pull, Severity::Critical,
        max_observed, EGR_DUTY_OBSERVED_TOLERANCE_PCT,
        R16.rationale_one_liner,
        Some("AGR_arwMEAB0KL: zero all cells in both banks; arwMLGRDKF: ≥850 mg/stroke."),
    )]
}

/// R17 — MAF deviation > 15 % sustained for > 2 s at warm cruise.
pub fn r17_maf_deviation(log: &ResampledLog, pull: &Pull) -> Vec<Finding> {
    if !has_all(log, &["maf_actual", "maf_spec"]) {
        return vec![make_skipped(&R17, pull, "channels maf_actual/maf_spec missing")];
    }
    let actual = match slice(log, pull, "maf_actual") { Some(v) => v, None => return vec![] };
    let spec = match slice(log, pull, "maf_spec") { Some(v) => v, None => return vec![] };
    let coolant = slice(log, pull, "coolant_c");
    let pedal = slice(log, pull, "tps_pct");
    let times = slice_time(log, pull);

    let n = actual.len();
    if n == 0 { return vec![]; }
    let dt = median_dt(times);
    let min_run = ((MAF_DEVIATION_MIN_DURATION_S / dt).round() as usize).max(1);

    let mut breach = vec![false; n];
    let mut worst: f64 = 0.0;
    for i in 0..n {
        // Apply cold-start and WOT exclusions.
        if let Some(c) = coolant {
            if c[i].is_finite() && c[i] < COLD_START_COOLANT_CUTOFF_C { continue; }
        }
        if let Some(p) = pedal {
            if p[i].is_finite() && p[i] >= WOT_PEDAL_CUTOFF_PCT { continue; }
        }
        if !actual[i].is_finite() || !spec[i].is_finite() || spec[i] <= 0.0 {
            continue;
        }
        let deviation = (actual[i] - spec[i]).abs() / spec[i];
        if deviation > MAF_DEVIATION_FRACTION {
            breach[i] = true;
            if deviation > worst { worst = deviation; }
        }
    }
    let mut found_run = false;
    let mut i = 0;
    while i < n {
        if breach[i] {
            let mut j = i;
            while j < n && breach[j] { j += 1; }
            if (j - i) >= min_run { found_run = true; break; }
            i = j;
        } else {
            i += 1;
        }
    }
    if !found_run { return vec![]; }
    vec![one(
        &R17, pull, Severity::Warn,
        worst * 100.0, MAF_DEVIATION_FRACTION * 100.0,
        R17.rationale_one_liner,
        Some("arwMLGRDKF: re-flatten ≥850 mg/stroke across all cells (Strategy B)."),
    )]
}

/// R17b — MAF actual exceeds spec by ≥50 mg with EGR=0 → info, "delete
/// is functional". Only fires when R17 does not (i.e. positive excess
/// is fine; negative-or-symmetric deviation is what R17 catches).
pub fn r17b_maf_excess_info(log: &ResampledLog, pull: &Pull) -> Vec<Finding> {
    if !has_all(log, &["maf_actual", "maf_spec", "egr_duty"]) {
        return vec![make_skipped(
            &R17B, pull, "channels maf_actual/maf_spec/egr_duty missing",
        )];
    }
    let actual = match slice(log, pull, "maf_actual") { Some(v) => v, None => return vec![] };
    let spec = match slice(log, pull, "maf_spec") { Some(v) => v, None => return vec![] };
    let egr = match slice(log, pull, "egr_duty") { Some(v) => v, None => return vec![] };

    let mut max_excess: f64 = 0.0;
    let mut any = false;
    for i in 0..actual.len() {
        if !actual[i].is_finite() || !spec[i].is_finite() || !egr[i].is_finite() {
            continue;
        }
        if egr[i] > EGR_DUTY_OBSERVED_TOLERANCE_PCT {
            // Don't claim "delete functional" when EGR is still active.
            return vec![];
        }
        let excess = actual[i] - spec[i];
        if excess >= MAF_EXCESS_INFO_MG {
            any = true;
            if excess > max_excess { max_excess = excess; }
        }
    }
    if !any { return vec![]; }
    vec![one(
        &R17B, pull, Severity::Info,
        max_excess, MAF_EXCESS_INFO_MG,
        R17B.rationale_one_liner,
        None,
    )]
}

/// R18 — Cruise-band SOI is at or above stock (within ±0.2°) AND
/// EGR=0 → info, recommend the −1.0° NVH retard.
pub fn r18_cruise_nvh(log: &ResampledLog, pull: &Pull) -> Vec<Finding> {
    if !has_all(log, &["soi_actual", "egr_duty", "rpm", "iq_requested"]) {
        return vec![make_skipped(&R18, pull, "channels for cruise NVH missing")];
    }
    let soi = match slice(log, pull, "soi_actual") { Some(v) => v, None => return vec![] };
    let egr = match slice(log, pull, "egr_duty") { Some(v) => v, None => return vec![] };
    let rpm = match slice(log, pull, "rpm") { Some(v) => v, None => return vec![] };
    let iq = match slice(log, pull, "iq_requested") { Some(v) => v, None => return vec![] };
    let coolant = slice(log, pull, "coolant_c");
    let pedal = slice(log, pull, "tps_pct");

    let mut samples_in_band = 0usize;
    let mut soi_at_or_above_stock = 0usize;
    for i in 0..soi.len() {
        if !soi[i].is_finite() || !rpm[i].is_finite() || !iq[i].is_finite() || !egr[i].is_finite() {
            continue;
        }
        if egr[i] > EGR_DUTY_OBSERVED_TOLERANCE_PCT { continue; }
        if !in_cruise_band(rpm[i], iq[i]) { continue; }
        if let Some(c) = coolant {
            if c[i].is_finite() && c[i] < WARM_COOLANT_MIN_C { continue; }
        }
        if let Some(p) = pedal {
            if p[i].is_finite() && p[i] > CRUISE_PEDAL_MAX_PCT { continue; }
        }
        samples_in_band += 1;
        // Stock cruise SOI is roughly 18-22° BTDC; "at or above stock"
        // here is a stand-in for "no retard applied yet". Use 18° as the
        // floor so any value at-or-above counts.
        if soi[i] >= 18.0 - 0.2 {
            soi_at_or_above_stock += 1;
        }
    }
    if samples_in_band == 0 || soi_at_or_above_stock == 0 {
        return vec![];
    }
    // Fire if a clear majority of the cruise samples show stock-or-above SOI.
    if soi_at_or_above_stock * 2 < samples_in_band {
        return vec![];
    }
    vec![one(
        &R18, pull, Severity::Info,
        soi_at_or_above_stock as f64, samples_in_band as f64,
        R18.rationale_one_liner,
        Some("SOI_warm_cruise: −1.0° BTDC in 1500-2500 rpm × 5-15 mg, warm SOI maps 0..3."),
    )]
}

/// R19 — DTC scan: any P0401..P0406 in `dtc_codes`.
pub fn r19_dtc_scan(log: &ResampledLog, pull: &Pull) -> Vec<Finding> {
    if pull.pull_id != 1 {
        return vec![];
    }
    if !log.data.contains_key("dtc_codes") {
        return vec![make_skipped(&R19, pull, "channel dtc_codes missing")];
    }
    // dtc_codes is encoded as one numeric value per sample where each row
    // is the integer suffix of a Pxxx code (e.g. 401 for P0401). Channels
    // are floats so we represent absent as NaN.
    let dtc = match log.get("dtc_codes") { Some(v) => v, None => return vec![] };
    let observed: Vec<u16> = dtc.iter().copied()
        .filter(|x| x.is_finite())
        .map(|x| x.round() as u16)
        .collect();
    if observed.is_empty() {
        return vec![];
    }
    // Map to P-codes and filter.
    let suspect: Vec<&'static str> = DTC_LIST_TO_SUPPRESS.iter()
        .copied()
        .filter(|code| {
            let n: u16 = code[1..].parse().unwrap_or(0);
            observed.contains(&n)
        })
        .collect();
    if suspect.is_empty() {
        return vec![];
    }
    let wiring_fault = suspect.iter().any(|c| DTC_WIRING_FAULTS.contains(c));
    let action = if wiring_fault {
        "P0403 → real solenoid wiring fault, investigate before suppressing."
    } else {
        "DTC_thresholds: widen P0401/P0402 plausibility windows (or zero enable flags)."
    };
    vec![one(
        &R19, pull, Severity::Warn,
        suspect.len() as f64, 0.0,
        R19.rationale_one_liner,
        Some(action),
    )]
}

/// Dispatch one rule against one pull, honouring the `LOW_RATE` flag for
/// rules that downgrade on slow logs.
pub fn dispatch(rule: &Rule, log: &ResampledLog, pull: &Pull, low_rate: bool) -> Vec<Finding> {
    match rule.id {
        "R01" => r01_underboost(log, pull),
        "R02" => r02_overboost_spike(log, pull),
        "R03" => r03_boost_target_excessive(log, pull),
        "R04" => r04_no_taper(log, pull),
        "R05" => r05_maf_below_spec(log, pull),
        "R06" => r06_lambda_floor(log, pull),
        "R07" => r07_peak_iq(log, pull),
        "R08" => r08_torque_ceiling(log, pull),
        "R09" => r09_soi_excess(log, pull, low_rate),
        "R10" => r10_eoi_late(log, pull, low_rate),
        "R11" => r11_coolant_low(log, pull),
        "R12" => r12_no_atm(log, pull),
        "R13" => r13_fuel_temp(log, pull),
        "R14" => r14_srcv(log, pull),
        "R15" => r15_limp(log, pull),
        "R16" => r16_egr_observed(log, pull),
        "R17" => r17_maf_deviation(log, pull),
        "R17b" => r17b_maf_excess_info(log, pull),
        "R18" => r18_cruise_nvh(log, pull),
        "R19" => r19_dtc_scan(log, pull),
        _ => Vec::new(),
    }
}
