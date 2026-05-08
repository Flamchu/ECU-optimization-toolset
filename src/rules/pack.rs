//! Rule pack for AMF / EDC15P+ — R01..R21 (v4, audit-reconciled).
//!
//! Each rule below carries its rationale as a docstring (per spec §6
//! implementation note) so it surfaces in `--help` and the Markdown
//! report.

use crate::platform::amf_edc15p::egr::{
    in_cruise_band, CRUISE_PEDAL_MAX_PCT, DTC_GROUP_B, DTC_LIST_TO_SUPPRESS,
    DTC_WIRING_FAULTS, EGR_DUTY_OBSERVED_TOLERANCE_PCT,
    IDLE_INSTABILITY_INFO_RPM_STD, IDLE_INSTABILITY_THRESHOLD_RPM_STD,
    IDLE_PEDAL_MAX_PCT, IDLE_WINDOW_MIN_S, MAF_DEVIATION_FRACTION,
    MAF_DEVIATION_MIN_DURATION_S, MAF_EXCESS_INFO_MG, WOT_PEDAL_CUTOFF_PCT,
};
use crate::platform::amf_edc15p::envelope::{
    clamp_low_pedal_slope, CAPS, DIESEL_AFR_STOICH, NM_PER_MG_IQ,
};
use crate::platform::amf_edc15p::stock_refs::stock_boost_at_rpm;
use crate::rules::base::{make_skipped, Finding, Rule, RuleScope, Severity};
use crate::util::timebase::ResampledLog;
use crate::util::Pull;

// ---------------------------------------------------------------------------
// Rule id enumeration (exhaustive — every variant must be dispatched).
// ---------------------------------------------------------------------------

/// All rule ids in canonical order. Used for exhaustive dispatch and the
/// acceptance test that asserts every variant is reachable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RuleId {
    /// R01 — Underboost.
    R01,
    /// R02 — Overboost spike.
    R02,
    /// R03 — Boost target excessive.
    R03,
    /// R04 — High-RPM boost not tapering.
    R04,
    /// R05 — MAF below spec.
    R05,
    /// R06 — Lambda floor breach.
    R06,
    /// R07 — Peak IQ above envelope.
    R07,
    /// R08 — Modelled torque above clutch.
    R08,
    /// R09 — SOI excess.
    R09,
    /// R10 — EOI late.
    R10,
    /// R11 — Coolant low during pull.
    R11,
    /// R12 — Atmospheric pressure missing.
    R12,
    /// R13 — Fuel temp high.
    R13,
    /// R14 — Smooth-running deviation.
    R14,
    /// R15 — Limp / N75 clamped.
    R15,
    /// R16 — EGR duty observed.
    R16,
    /// R17 — MAF deviation post-delete.
    R17,
    /// R18 — Cruise SOI NVH.
    R18,
    /// R19 — DTC scan.
    R19,
    /// R20 — Cruise spec-MAF excess.
    R20,
    /// R21 — Idle stability.
    R21,
    /// R22 — Low-pedal IQ slope excessive (driveability).
    R22,
    /// R23 — Sustained-pull coolant trend (thermal).
    R23,
}

/// Iteration order for the rule pack.
pub const ALL_RULE_IDS: &[RuleId] = &[
    RuleId::R01, RuleId::R02, RuleId::R03, RuleId::R04, RuleId::R05,
    RuleId::R06, RuleId::R07, RuleId::R08, RuleId::R09, RuleId::R10,
    RuleId::R11, RuleId::R12, RuleId::R13, RuleId::R14, RuleId::R15,
    RuleId::R16, RuleId::R17, RuleId::R18, RuleId::R19, RuleId::R20,
    RuleId::R21, RuleId::R22, RuleId::R23,
];

impl RuleId {
    /// Short string id used everywhere outside the dispatcher.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::R01 => "R01", Self::R02 => "R02", Self::R03 => "R03",
            Self::R04 => "R04", Self::R05 => "R05", Self::R06 => "R06",
            Self::R07 => "R07", Self::R08 => "R08", Self::R09 => "R09",
            Self::R10 => "R10", Self::R11 => "R11", Self::R12 => "R12",
            Self::R13 => "R13", Self::R14 => "R14", Self::R15 => "R15",
            Self::R16 => "R16", Self::R17 => "R17", Self::R18 => "R18",
            Self::R19 => "R19", Self::R20 => "R20", Self::R21 => "R21",
            Self::R22 => "R22", Self::R23 => "R23",
        }
    }

    /// Look up the static [`Rule`] descriptor.
    pub fn rule(self) -> &'static Rule {
        match self {
            Self::R01 => &R01, Self::R02 => &R02, Self::R03 => &R03,
            Self::R04 => &R04, Self::R05 => &R05, Self::R06 => &R06,
            Self::R07 => &R07, Self::R08 => &R08, Self::R09 => &R09,
            Self::R10 => &R10, Self::R11 => &R11, Self::R12 => &R12,
            Self::R13 => &R13, Self::R14 => &R14, Self::R15 => &R15,
            Self::R16 => &R16, Self::R17 => &R17, Self::R18 => &R18,
            Self::R19 => &R19, Self::R20 => &R20, Self::R21 => &R21,
            Self::R22 => &R22, Self::R23 => &R23,
        }
    }
}

// ---------------------------------------------------------------------------
// Rule descriptors (R01..R21)
// ---------------------------------------------------------------------------

/// R01 — Underboost.
pub const R01: Rule = Rule {
    id: "R01",
    severity: Severity::Warn,
    scope: RuleScope::PerPull,
    rationale_one_liner:
        "Persistent underboost: leak, dirty MAF, sticky wastegate, or LDRXN ramp too steep.",
    recommended_delta_ref: Some("LDRXN/N75_duty"),
    requires_channels: &["rpm", "boost_actual", "boost_spec"],
    requires_groups: &["011"],
};

/// R02 — Overboost spike.
pub const R02: Rule = Rule {
    id: "R02",
    severity: Severity::Critical,
    scope: RuleScope::PerPull,
    rationale_one_liner:
        "Garrett GT1544S sustained over 2150 mbar pushes shaft past the right edge of the compressor map → overspeed.",
    recommended_delta_ref: Some("LDRXN: lower target"),
    requires_channels: &["boost_actual", "boost_spec"],
    requires_groups: &["011"],
};

/// R03 — Boost target excessive.
pub const R03: Rule = Rule {
    id: "R03",
    severity: Severity::Critical,
    scope: RuleScope::PerPull,
    rationale_one_liner: "Hard envelope ceiling for Garrett GT1544S longevity.",
    recommended_delta_ref: Some("LDRXN: rpm 2000-3500"),
    requires_channels: &["rpm", "boost_spec"],
    requires_groups: &["011"],
};

/// R04 — High-RPM boost not tapering.
pub const R04: Rule = Rule {
    id: "R04",
    severity: Severity::Warn,
    scope: RuleScope::PerPull,
    rationale_one_liner:
        "Garrett GT1544S is choke-flow-limited above 4000 rpm; back off to keep it in the efficiency island.",
    recommended_delta_ref: Some("LDRXN taper rpm 4000-4500"),
    requires_channels: &["rpm", "boost_spec"],
    requires_groups: &["011"],
};

/// R05 — MAF below spec.
pub const R05: Rule = Rule {
    id: "R05",
    severity: Severity::Warn,
    scope: RuleScope::PerPull,
    rationale_one_liner:
        "MAF drift, dirty intake, boost leak, or MAF aging — fueling decisions become wrong.",
    recommended_delta_ref: Some("MLHFM (only if MAF replaced)"),
    requires_channels: &["maf_actual", "maf_spec"],
    requires_groups: &["003"],
};

/// R06 — Lambda floor breach (v4 floor: 1.05).
pub const R06: Rule = Rule {
    id: "R06",
    severity: Severity::Critical,
    scope: RuleScope::PerPull,
    rationale_one_liner:
        "Below λ = 1.05 = past stoich → incomplete combustion → EGT spike → ring-land cracks.",
    recommended_delta_ref: Some("Smoke_IQ_by_MAF + Smoke_IQ_by_MAP: enforce λ ≥ 1.05"),
    requires_channels: &["maf_actual", "iq_requested"],
    requires_groups: &["003", "008"],
};

/// R07 — Peak IQ above sane envelope (v4 cap: 54 mg/stroke).
pub const R07: Rule = Rule {
    id: "R07",
    severity: Severity::Critical,
    scope: RuleScope::PerPull,
    rationale_one_liner:
        "Above 54 mg/stroke the PD75 nozzle duration headroom and LUK clutch torque ceiling run out.",
    recommended_delta_ref: Some("Driver_Wish + Duration"),
    requires_channels: &["iq_requested"],
    requires_groups: &["008"],
};

/// R08 — Modelled torque above clutch ceiling.
pub const R08: Rule = Rule {
    id: "R08",
    severity: Severity::Critical,
    scope: RuleScope::PerPull,
    rationale_one_liner:
        "LUK SMF engineering ceiling 240 Nm (LUK does not publish a rating). Above this the clutch slips within weeks.",
    recommended_delta_ref: Some("Torque_Limiter: clamp peak ≤ 240 Nm"),
    requires_channels: &["iq_requested"],
    requires_groups: &["008"],
};

/// R09 — SOI excess. Critical → Warn under LOW_RATE.
pub const R09: Rule = Rule {
    id: "R09",
    severity: Severity::Critical,
    scope: RuleScope::PerPull,
    rationale_one_liner:
        "Beyond 26° BTDC at IQ ≥ 30 mg, peak cylinder pressure migrates ahead of TDC → piston crown stress.",
    recommended_delta_ref: Some("SOI: cap at 26° BTDC absolute"),
    requires_channels: &["soi_actual", "iq_requested"],
    requires_groups: &["020", "008"],
};

/// R10 — EOI late. Warn baseline; no LOW_RATE downgrade because Warn is
/// already the lowest non-info severity.
pub const R10: Rule = Rule {
    id: "R10",
    severity: Severity::Warn,
    scope: RuleScope::PerPull,
    rationale_one_liner:
        "Combustion past 10° ATDC dumps unburned heat into the turbine → high EGT, poor BSFC. Duration is a screening heuristic.",
    recommended_delta_ref: Some("SOI/Duration: re-balance"),
    requires_channels: &["soi_actual", "iq_requested", "rpm"],
    requires_groups: &["020", "008"],
};

/// R11 — Coolant too low for pull.
pub const R11: Rule = Rule {
    id: "R11",
    severity: Severity::Info,
    scope: RuleScope::PerPull,
    rationale_one_liner:
        "EDC15P+ uses cold SOI map below 80 °C — pull data is not representative of warm calibration.",
    recommended_delta_ref: None,
    requires_channels: &["coolant_c"],
    requires_groups: &["001"],
};

/// R12 — Atmospheric correction missing.
pub const R12: Rule = Rule {
    id: "R12",
    severity: Severity::Info,
    scope: RuleScope::PerPull,
    rationale_one_liner:
        "Without ambient pressure capture (group 010 key-on/engine-off), altitude derate cannot be assessed.",
    recommended_delta_ref: None,
    requires_channels: &["atm_pressure"],
    requires_groups: &["010"],
};

/// R13 — Fuel temp high.
pub const R13: Rule = Rule {
    id: "R13",
    severity: Severity::Warn,
    scope: RuleScope::PerPull,
    rationale_one_liner:
        "High fuel temp = lower density = lower delivered IQ for same duration → boost overshoots fuelling.",
    recommended_delta_ref: None,
    requires_channels: &["fuel_temp_c"],
    requires_groups: &["013"],
};

/// R14 — Smooth-running deviation.
pub const R14: Rule = Rule {
    id: "R14",
    severity: Severity::Warn,
    scope: RuleScope::PerPull,
    rationale_one_liner:
        "Worn injector cam lobe (PD weak point) or failing injector. Tuning a sick engine kills it faster.",
    recommended_delta_ref: None,
    requires_channels: &["srcv_cyl1", "srcv_cyl2", "srcv_cyl3"],
    requires_groups: &["013"],
};

/// R15 — Limp / N75 clamped.
pub const R15: Rule = Rule {
    id: "R15",
    severity: Severity::Warn,
    scope: RuleScope::PerPull,
    rationale_one_liner: "ECU is in limp mode — log is not valid for tuning.",
    recommended_delta_ref: None,
    requires_channels: &["n75_duty"],
    requires_groups: &["011"],
};

// ---------------------------------------------------------------------------
// v4 EGR-delete + idle-stability additions (R16..R21)
// ---------------------------------------------------------------------------

/// R16 — EGR duty observed (delete not applied). Global scope.
pub const R16: Rule = Rule {
    id: "R16",
    severity: Severity::Critical,
    scope: RuleScope::Global,
    rationale_one_liner:
        "v4 mandates a software EGR delete. Any EGR duty > 2 % anywhere in the log means the delete \
         was not flashed, was applied to only one bank, or was overridden by spec-MAF polarity.",
    recommended_delta_ref: Some("AGR_arwMEAB0KL + AGR_arwMEAB1KL + arwMLGRDKF"),
    requires_channels: &["egr_duty"],
    requires_groups: &["003"],
};

/// R17 — MAF deviation post-delete (cruise, warm).
pub const R17: Rule = Rule {
    id: "R17",
    severity: Severity::Warn,
    scope: RuleScope::PerPull,
    rationale_one_liner:
        "Cruise MAF should track spec-MAF closely (Strategy A) or sit above it (Strategy B). >15 % deviation \
         sustained for >2 s indicates the spec-MAF map was not rescaled correctly. Reads `pedal_pct` (driver wish).",
    recommended_delta_ref: Some("arwMLGRDKF: re-flatten ≥850 mg/stroke"),
    requires_channels: &["maf_actual", "maf_spec", "pedal_pct"],
    requires_groups: &["003"],
};

/// R18 — Cruise-band SOI NVH check.
pub const R18: Rule = Rule {
    id: "R18",
    severity: Severity::Info,
    scope: RuleScope::PerPull,
    rationale_one_liner:
        "Cruise-band SOI is at stock with EGR off. Premixed phase is faster without inert charge → NVH bump. \
         Apply −1.0° SOI retard to warm SOI maps 0..3 in the cruise band only if subjective NVH is objectionable.",
    recommended_delta_ref: Some("SOI_warm_cruise"),
    requires_channels: &["soi_actual", "egr_duty", "rpm", "iq_requested"],
    requires_groups: &["020", "003", "008"],
};

/// R19 — DTC scan check. Global scope; reads from `VcdsLog.dtcs`.
pub const R19: Rule = Rule {
    id: "R19",
    severity: Severity::Warn,
    scope: RuleScope::Global,
    rationale_one_liner:
        "P0401/P0402/P0403 (Group A, real on AMF) post-flash mean the DTC was not suppressed (P0401/P0402) \
         or the EGR solenoid has a real wiring fault (P0403 — investigate, do NOT just suppress). \
         P0404/P0405/P0406 (Group B) should not appear on AMF and indicate code-list error or non-AMF ECU.",
    recommended_delta_ref: Some("DTC_thresholds"),
    requires_channels: &[],
    requires_groups: &[],
};

/// R20 — Cruise spec-MAF excess (was R17b in v3 — promoted to R20 for
/// flat numbering).
pub const R20: Rule = Rule {
    id: "R20",
    severity: Severity::Info,
    scope: RuleScope::PerPull,
    rationale_one_liner:
        "Post-delete with Strategy-B saturation, MAF actual exceeding spec by ≥50 mg with EGR=0 confirms the \
         delete is functional and the saturation is harmless.",
    recommended_delta_ref: None,
    requires_channels: &["maf_actual", "maf_spec", "egr_duty"],
    requires_groups: &["003"],
};

/// R21 — Idle stability. Global scope; evaluates the warm idle window
/// across the whole log.
pub const R21: Rule = Rule {
    id: "R21",
    severity: Severity::Warn,
    scope: RuleScope::Global,
    rationale_one_liner:
        "RPM σ > 25 over a 30-s warm-idle window catches injector / mech imbalance survival. \
         Severity downgrades to Info if the window is < 30 s (insufficient evidence).",
    recommended_delta_ref: Some("Idle_fuel"),
    requires_channels: &["rpm", "coolant_c"],
    requires_groups: &["001"],
};

/// R22 — Low-pedal IQ slope (driveability). Per-pull, Warn baseline
/// (no LOW_RATE downgrade — mirrors R10).
pub const R22: Rule = Rule {
    id: "R22",
    severity: Severity::Warn,
    scope: RuleScope::PerPull,
    rationale_one_liner:
        "Low-pedal Driver_Wish slope ramps IQ too aggressively (off-idle lunge). \
         Linear regression of iq_mg against pedal_pct in the 5..25 % band; fires \
         Warn if the slope exceeds the cap, or if it is markedly steeper than the \
         25..80 % mid-band.",
    recommended_delta_ref: Some("Driver_Wish_low_pedal"),
    requires_channels: &["pedal_pct", "iq_requested", "rpm"],
    requires_groups: &["008"],
};

/// R23 — Sustained-pull coolant trend (thermal). Per-pull. Pass / Info /
/// Warn ladder; never Critical.
pub const R23: Rule = Rule {
    id: "R23",
    severity: Severity::Info,
    scope: RuleScope::PerPull,
    rationale_one_liner:
        "Coolant rose substantially during a sustained pull and approached or \
         exceeded the warn-level peak. Verifies that the cooling-fan threshold \
         and run-on tune is doing useful work.",
    recommended_delta_ref: Some("Fan_thresholds"),
    requires_channels: &["coolant_c", "rpm"],
    requires_groups: &["001", "011"],
};

/// All rules in canonical order (R01..R23).
pub const ALL_RULES: &[&Rule] = &[
    &R01, &R02, &R03, &R04, &R05, &R06, &R07,
    &R08, &R09, &R10, &R11, &R12, &R13, &R14, &R15,
    &R16, &R17, &R18, &R19, &R20, &R21, &R22, &R23,
];

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

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

fn finite_mean_std(xs: &[f64]) -> Option<(f64, f64)> {
    let finite: Vec<f64> = xs.iter().copied().filter(|x| x.is_finite()).collect();
    if finite.is_empty() { return None; }
    let n = finite.len() as f64;
    let mean = finite.iter().sum::<f64>() / n;
    let var = finite.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / n;
    Some((mean, var.sqrt()))
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

/// PD injection-duration screening heuristic. Documented in spec §6 R10
/// as a screening estimator only — it is NOT a physical PD model.
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

/// R01 — actual < spec − 150 mbar for ≥ 1.0 s above 2000 rpm.
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
    if !breach.iter().any(|&b| b) { return vec![]; }
    let dt = median_dt(times);
    let min_run = ((1.0 / dt).round() as usize).max(1);
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

/// R02 — actual > spec + 200 mbar OR > 2200 mbar abs.
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

/// R03 — any spec > 2150 mbar abs, or > stock+250.
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
    if near_3000.is_empty() || near_4500.is_empty() { return vec![]; }
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

/// R06 — Lambda floor breach: any sample where λ < 1.05.
pub fn r06_lambda_floor(log: &ResampledLog, pull: &Pull) -> Vec<Finding> {
    if !has_all(log, &["maf_actual", "iq_requested"]) {
        return vec![make_skipped(&R06, pull, "channels maf_actual/iq_requested missing")];
    }
    let maf = match slice(log, pull, "maf_actual") { Some(v) => v, None => return vec![] };
    let iq = match slice(log, pull, "iq_requested") { Some(v) => v, None => return vec![] };
    let n = maf.len();
    let mut min_lambda = f64::INFINITY;
    for i in 0..n {
        if !maf[i].is_finite() || !iq[i].is_finite() || iq[i] <= 0.0 { continue; }
        let lam = maf[i] / (iq[i] * DIESEL_AFR_STOICH);
        if lam < min_lambda { min_lambda = lam; }
    }
    if !min_lambda.is_finite() || min_lambda >= CAPS.lambda_floor { return vec![]; }
    vec![one(
        &R06, pull, Severity::Critical, min_lambda, CAPS.lambda_floor,
        R06.rationale_one_liner,
        Some("Smoke_IQ_by_MAF/MAP: raise IQ cap so λ ≥ 1.05 at this MAF."),
    )]
}

/// R07 — IQ requested > peak_iq_mg cap.
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
        Some("Driver_Wish: cap WOT request at envelope IQ cap."),
    )]
}

/// R08 — Modelled flywheel torque > clutch ceiling.
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

/// R09 — soi_actual > 26° BTDC at any IQ ≥ 30 mg. Critical → Warn under
/// `LOW_RATE` because SOI transients can be missed at slow VCDS rates.
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

/// R10 — Computed EOI = SOI − duration_model > 10° ATDC. v4: no
/// LOW_RATE downgrade (Warn baseline).
pub fn r10_eoi_late(log: &ResampledLog, pull: &Pull) -> Vec<Finding> {
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
        if eoi_atdc > CAPS.eoi_max_atdc && eoi_atdc > worst { worst = eoi_atdc; }
    }
    if !worst.is_finite() { return vec![]; }
    vec![one(
        &R10, pull, Severity::Warn, worst, CAPS.eoi_max_atdc,
        &format!("{} (duration modelled, not measured)", R10.rationale_one_liner),
        Some("SOI/Duration: tighten duration before extending IQ further."),
    )]
}

/// R11 — Coolant < `coolant_pull_min_c` during pull.
pub fn r11_coolant_low(log: &ResampledLog, pull: &Pull) -> Vec<Finding> {
    if !log.has("coolant_c") {
        return vec![make_skipped(&R11, pull, "channel coolant_c missing")];
    }
    let c = match slice(log, pull, "coolant_c") { Some(v) => v, None => return vec![] };
    let min = finite_min(c).unwrap_or(f64::INFINITY);
    if min >= CAPS.coolant_pull_min_c { return vec![]; }
    vec![one(
        &R11, pull, Severity::Info, min, CAPS.coolant_pull_min_c,
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

/// R16 — EGR duty observed anywhere in the log (global scope).
pub fn r16_egr_observed(log: &ResampledLog, pull: &Pull) -> Vec<Finding> {
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
        Some("AGR_arwMEAB0KL + AGR_arwMEAB1KL: zero both banks; arwMLGRDKF: ≥850 mg/stroke."),
    )]
}

/// R17 — MAF deviation > 15 % sustained for > 2 s at warm cruise. Reads
/// `pedal_pct` (driver-wish), NOT `tps_pct` (anti-shudder valve). When
/// `pedal_pct` is missing, R17 is fail-safe SKIPPED rather than firing.
pub fn r17_maf_deviation(log: &ResampledLog, pull: &Pull) -> Vec<Finding> {
    if !has_all(log, &["maf_actual", "maf_spec"]) {
        return vec![make_skipped(&R17, pull, "channels maf_actual/maf_spec missing")];
    }
    if !log.has("pedal_pct") {
        return vec![make_skipped(
            &R17, pull,
            "pedal_pct channel missing — refusing to fire (would false-positive at WOT without driver-wish)",
        )];
    }
    let actual = match slice(log, pull, "maf_actual") { Some(v) => v, None => return vec![] };
    let spec = match slice(log, pull, "maf_spec") { Some(v) => v, None => return vec![] };
    let coolant = slice(log, pull, "coolant_c");
    let pedal = match slice(log, pull, "pedal_pct") { Some(v) => v, None => return vec![] };
    let times = slice_time(log, pull);

    let n = actual.len();
    if n == 0 { return vec![]; }
    let dt = median_dt(times);
    let min_run = ((MAF_DEVIATION_MIN_DURATION_S / dt).round() as usize).max(1);

    let mut breach = vec![false; n];
    let mut worst: f64 = 0.0;
    for i in 0..n {
        if let Some(c) = coolant {
            if c[i].is_finite() && c[i] < CAPS.warm_coolant_min_c { continue; }
        }
        if pedal[i].is_finite() && pedal[i] >= WOT_PEDAL_CUTOFF_PCT { continue; }
        if !actual[i].is_finite() || !spec[i].is_finite() || spec[i] <= 0.0 { continue; }
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
    let pedal = slice(log, pull, "pedal_pct");

    let mut samples_in_band = 0usize;
    let mut soi_at_or_above_stock = 0usize;
    for i in 0..soi.len() {
        if !soi[i].is_finite() || !rpm[i].is_finite() || !iq[i].is_finite() || !egr[i].is_finite() {
            continue;
        }
        if egr[i] > EGR_DUTY_OBSERVED_TOLERANCE_PCT { continue; }
        if !in_cruise_band(rpm[i], iq[i]) { continue; }
        if let Some(c) = coolant {
            if c[i].is_finite() && c[i] < CAPS.warm_coolant_min_c { continue; }
        }
        if let Some(p) = pedal {
            if p[i].is_finite() && p[i] > CRUISE_PEDAL_MAX_PCT { continue; }
        }
        samples_in_band += 1;
        if soi[i] >= 18.0 - 0.2 { soi_at_or_above_stock += 1; }
    }
    if samples_in_band == 0 || soi_at_or_above_stock == 0 { return vec![]; }
    if soi_at_or_above_stock * 2 < samples_in_band { return vec![]; }
    vec![one(
        &R18, pull, Severity::Info,
        soi_at_or_above_stock as f64, samples_in_band as f64,
        R18.rationale_one_liner,
        Some("SOI_warm_cruise: −1.0° BTDC in 1500-2500 rpm × 5-15 mg, warm SOI maps 0..3."),
    )]
}

/// R19 — DTC scan from sidecar file. Global scope. `dtcs` is the parsed
/// `Vec<String>` from `<base>.dtc.txt` (see [`crate::ingest::dtc`]).
/// Returns SKIPPED when the sidecar is absent / empty.
pub fn r19_dtc_scan(dtcs: &[String], pull: &Pull) -> Vec<Finding> {
    if dtcs.is_empty() {
        return vec![make_skipped(&R19, pull, "no DTC scan provided (sidecar missing or empty)")];
    }
    let suspect: Vec<&'static str> = DTC_LIST_TO_SUPPRESS.iter()
        .copied()
        .filter(|code| dtcs.iter().any(|d| d.eq_ignore_ascii_case(code)))
        .collect();
    if suspect.is_empty() { return vec![]; }
    let wiring_fault = suspect.iter().any(|c| DTC_WIRING_FAULTS.contains(c));
    let group_b_only = suspect.iter().all(|c| DTC_GROUP_B.contains(c));
    let action = if wiring_fault {
        "P0403 → real EGR solenoid wiring fault, investigate before suppressing."
    } else if group_b_only {
        "P0404/P0405/P0406 should not appear on AMF — verify DAMOS code-list and ECU file."
    } else {
        "DTC_thresholds: widen P0401/P0402 plausibility windows (or zero enable flags)."
    };
    let rationale = format!(
        "{} (observed: {})",
        R19.rationale_one_liner,
        suspect.join(", "),
    );
    vec![one(
        &R19, pull, Severity::Warn,
        suspect.len() as f64, 0.0,
        &rationale,
        Some(action),
    )]
}

/// R20 — MAF actual exceeds spec by ≥50 mg with EGR=0 (was R17b).
pub fn r20_maf_excess_info(log: &ResampledLog, pull: &Pull) -> Vec<Finding> {
    if !has_all(log, &["maf_actual", "maf_spec", "egr_duty"]) {
        return vec![make_skipped(
            &R20, pull, "channels maf_actual/maf_spec/egr_duty missing",
        )];
    }
    let actual = match slice(log, pull, "maf_actual") { Some(v) => v, None => return vec![] };
    let spec = match slice(log, pull, "maf_spec") { Some(v) => v, None => return vec![] };
    let egr = match slice(log, pull, "egr_duty") { Some(v) => v, None => return vec![] };

    let mut max_excess: f64 = 0.0;
    let mut any = false;
    for i in 0..actual.len() {
        if !actual[i].is_finite() || !spec[i].is_finite() || !egr[i].is_finite() { continue; }
        if egr[i] > EGR_DUTY_OBSERVED_TOLERANCE_PCT { return vec![]; }
        let excess = actual[i] - spec[i];
        if excess >= MAF_EXCESS_INFO_MG {
            any = true;
            if excess > max_excess { max_excess = excess; }
        }
    }
    if !any { return vec![]; }
    vec![one(
        &R20, pull, Severity::Info,
        max_excess, MAF_EXCESS_INFO_MG,
        R20.rationale_one_liner,
        None,
    )]
}

/// Linear regression slope of `y` on `x`. Returns `None` if fewer than
/// two distinct samples or if denominator is zero. Pure helper.
fn linreg_slope(xs: &[f64], ys: &[f64]) -> Option<f64> {
    debug_assert_eq!(xs.len(), ys.len());
    let n = xs.len();
    if n < 2 { return None; }
    let inv_n = 1.0 / n as f64;
    let mean_x = xs.iter().sum::<f64>() * inv_n;
    let mean_y = ys.iter().sum::<f64>() * inv_n;
    let mut num = 0.0;
    let mut den = 0.0;
    for i in 0..n {
        let dx = xs[i] - mean_x;
        num += dx * (ys[i] - mean_y);
        den += dx * dx;
    }
    if den.abs() < f64::EPSILON {
        return None;
    }
    Some(num / den)
}

/// R22 — Low-pedal IQ slope rule.
///
/// Filters samples to the warm, off-idle, on-engine band, computes a
/// linear regression of `iq_requested` against `pedal_pct` in the
/// low-pedal sub-band, and compares both the absolute slope and the
/// low/mid ratio against the envelope caps.
pub fn r22_low_pedal_slope(log: &ResampledLog, pull: &Pull) -> Vec<Finding> {
    if !log.has("pedal_pct") {
        return vec![make_skipped(
            &R22, pull,
            "pedal_pct channel unavailable; R22 requires a pedal sweep",
        )];
    }
    if !has_all(log, &["iq_requested", "rpm"]) {
        return vec![make_skipped(&R22, pull, "channels iq_requested/rpm missing")];
    }
    let pedal = match slice(log, pull, "pedal_pct") { Some(v) => v, None => return vec![] };
    let iq = match slice(log, pull, "iq_requested") { Some(v) => v, None => return vec![] };
    let rpm = match slice(log, pull, "rpm") { Some(v) => v, None => return vec![] };
    let coolant = slice(log, pull, "coolant_c");

    let creep = f64::from(CAPS.low_pedal_idle_creep_pct);
    let band_top = f64::from(CAPS.low_pedal_band_top_pct);
    let mid_top = 80.0;
    let warm = CAPS.warm_coolant_min_c;

    let mut low_x: Vec<f64> = Vec::new();
    let mut low_y: Vec<f64> = Vec::new();
    let mut mid_x: Vec<f64> = Vec::new();
    let mut mid_y: Vec<f64> = Vec::new();

    for i in 0..pedal.len() {
        if !pedal[i].is_finite() || !iq[i].is_finite() || !rpm[i].is_finite() {
            continue;
        }
        if rpm[i] < 1000.0 { continue; }
        if let Some(c) = coolant {
            if c[i].is_finite() && c[i] < warm { continue; }
        }
        let p = pedal[i];
        if p > creep && p <= band_top {
            low_x.push(p);
            low_y.push(iq[i]);
        } else if p > band_top && p <= mid_top {
            mid_x.push(p);
            mid_y.push(iq[i]);
        }
    }

    if low_x.len() < 30 {
        return vec![make_skipped(
            &R22, pull,
            "fewer than 30 samples in low-pedal band (5..25 %); pedal sweep too sparse",
        )];
    }

    let slope_low_raw = match linreg_slope(&low_x, &low_y) {
        Some(s) if s.is_finite() => s,
        _ => return vec![make_skipped(&R22, pull, "low-band regression denominator zero")],
    };
    let slope_low = clamp_low_pedal_slope(slope_low_raw);
    let slope_mid = linreg_slope(&mid_x, &mid_y).filter(|s| s.is_finite()).unwrap_or(0.0);

    let abs_breach = slope_low > CAPS.low_pedal_slope_max_mg_per_pct;
    let ratio_breach = slope_mid > 0.0
        && slope_low / slope_mid > CAPS.low_pedal_slope_ratio_max;

    if !abs_breach && !ratio_breach {
        return vec![];
    }

    let trigger = if abs_breach { "absolute" } else { "ratio" };
    let rationale = format!(
        "{} slope_low={:.3} mg/pct, slope_mid={:.3} mg/pct over {} low-band / {} mid-band samples \
         ({trigger} test)",
        R22.rationale_one_liner,
        slope_low, slope_mid, low_x.len(), mid_x.len(),
    );
    vec![one(
        &R22, pull, Severity::Warn,
        slope_low, CAPS.low_pedal_slope_max_mg_per_pct,
        &rationale,
        Some("Driver_Wish_low_pedal: flatten 5..25 % pedal column band; preserve idle creep ≤ 5 %."),
    )]
}

/// R23 — Sustained-pull coolant trend rule.
pub fn r23_coolant_trend(log: &ResampledLog, pull: &Pull) -> Vec<Finding> {
    if !log.has("coolant_c") {
        return vec![make_skipped(&R23, pull, "channel coolant_c missing")];
    }
    if !log.has("rpm") {
        return vec![make_skipped(&R23, pull, "channel rpm missing")];
    }
    let coolant = match slice(log, pull, "coolant_c") { Some(v) => v, None => return vec![] };
    let rpm = match slice(log, pull, "rpm") { Some(v) => v, None => return vec![] };
    let pedal = slice(log, pull, "pedal_pct");

    if pull.duration_s() < 5.0 {
        return vec![make_skipped(&R23, pull, "pull duration < 5 s")];
    }
    let max_rpm = finite_max(rpm).unwrap_or(0.0);
    if max_rpm < 2500.0 {
        return vec![make_skipped(&R23, pull, "rpm never exceeds 2500 in pull window")];
    }

    let n = coolant.len();
    let mut included: Vec<f64> = Vec::with_capacity(n);
    for i in 0..n {
        if !coolant[i].is_finite() || !rpm[i].is_finite() { continue; }
        if rpm[i] < 2500.0 { continue; }
        if let Some(p) = pedal {
            if p[i].is_finite() && p[i] < 60.0 { continue; }
        }
        included.push(coolant[i]);
    }
    // If the pedal-gated set is too sparse, fall back to the rpm-gated set.
    let series: Vec<f64> = if included.len() >= 5 {
        included
    } else {
        coolant.iter().copied()
            .zip(rpm.iter().copied())
            .filter(|(c, r)| c.is_finite() && r.is_finite() && *r >= 2500.0)
            .map(|(c, _)| c)
            .collect()
    };
    if series.len() < 2 {
        return vec![make_skipped(&R23, pull, "fewer than 2 finite coolant samples in pull window")];
    }
    let t_min = finite_min(&series).unwrap_or(0.0);
    let t_peak = finite_max(&series).unwrap_or(0.0);
    let dt = t_peak - t_min;

    let arm_c = f64::from(CAPS.r23_coolant_rise_arm_c);
    let warn_c = f64::from(CAPS.r23_coolant_peak_warn_c);

    if dt < arm_c {
        // Rule armed but quiet — emit a low-noise Info pass token only
        // when we have meaningful evidence.
        return vec![];
    }
    let severity = if t_peak >= warn_c { Severity::Warn } else { Severity::Info };
    let rationale = format!(
        "T_coolant rose by {:.1} °C and peaked at {:.1} °C during a sustained pull \
         (>{:.0} °C arms; ≥{:.0} °C warns). Verify Fan_thresholds + Fan_run_on are in effect.",
        dt, t_peak, arm_c, warn_c,
    );
    let action = if matches!(severity, Severity::Warn) {
        "Fan_thresholds: lower stage-1 on/off; Fan_run_on: extend post-key-off run-on."
    } else {
        "Fan_thresholds / Fan_run_on apply unconditionally; verify the flash actually wrote them."
    };
    vec![one(
        &R23, pull, severity,
        t_peak, warn_c, &rationale,
        Some(action),
    )]
}

/// R21 — Idle stability. Global scope; evaluates every warm-idle window
/// (coolant ≥ warm threshold, pedal ≤ idle, vehicle_speed = 0 if known).
/// Severity Warn at σ > 25; downgrades to Info if window < 30 s OR
/// σ > 15 (stricter screening).
pub fn r21_idle_stability(log: &ResampledLog, pull: &Pull) -> Vec<Finding> {
    if !has_all(log, &["rpm", "coolant_c"]) {
        return vec![make_skipped(&R21, pull, "channels rpm/coolant_c missing")];
    }
    let rpm = match log.get("rpm") { Some(v) => v, None => return vec![] };
    let coolant = match log.get("coolant_c") { Some(v) => v, None => return vec![] };
    let pedal = log.get("pedal_pct");
    let speed = log.get("vehicle_speed");
    let iq = log.get("iq_requested").or_else(|| log.get("iq_actual"));

    let n = log.time.len();
    if n == 0 { return vec![]; }

    // Collect contiguous warm-idle samples: pedal ≤ IDLE_PEDAL_MAX_PCT,
    // VSS = 0 if available, IQ ≤ IDLE_IQ_MAX_MG fallback when no pedal.
    let mut idle_rpm: Vec<f64> = Vec::new();
    let mut window_seconds: f64 = 0.0;
    let times = log.time.as_slice();
    let mut last_t: Option<f64> = None;

    for i in 0..n {
        if !rpm[i].is_finite() { continue; }
        if i >= coolant.len() || !coolant[i].is_finite() || coolant[i] < CAPS.warm_coolant_min_c {
            continue;
        }
        let pedal_ok = match pedal {
            Some(p) if i < p.len() && p[i].is_finite() => p[i] <= IDLE_PEDAL_MAX_PCT,
            _ => match iq {
                Some(q) if i < q.len() && q[i].is_finite() => q[i] <= 8.0,
                _ => true,
            },
        };
        if !pedal_ok { continue; }
        if let Some(s) = speed {
            if i < s.len() && s[i].is_finite() && s[i] > 1.0 { continue; }
        }
        idle_rpm.push(rpm[i]);
        if let Some(prev) = last_t {
            let dt = times[i] - prev;
            if dt.is_finite() && (0.0..1.0).contains(&dt) {
                window_seconds += dt;
            }
        }
        last_t = Some(times[i]);
    }

    if idle_rpm.is_empty() { return vec![]; }
    let Some((_, std)) = finite_mean_std(&idle_rpm) else { return vec![]; };

    if std <= IDLE_INSTABILITY_INFO_RPM_STD { return vec![]; }

    let severity = if std > IDLE_INSTABILITY_THRESHOLD_RPM_STD && window_seconds >= IDLE_WINDOW_MIN_S {
        Severity::Warn
    } else {
        Severity::Info
    };
    let threshold = if matches!(severity, Severity::Warn) {
        IDLE_INSTABILITY_THRESHOLD_RPM_STD
    } else {
        IDLE_INSTABILITY_INFO_RPM_STD
    };
    let extra = if window_seconds < IDLE_WINDOW_MIN_S {
        format!(
            " (window {window_seconds:.1}s < {IDLE_WINDOW_MIN_S:.0}s — downgraded to info)"
        )
    } else {
        String::new()
    };
    let rationale = format!("{}{extra}", R21.rationale_one_liner);
    vec![one(
        &R21, pull, severity, std, threshold, &rationale,
        Some("Idle_fuel: −1.5 mg/stroke at warm idle (only when R21 fires)."),
    )]
}

// ---------------------------------------------------------------------------
// Dispatch — exhaustive over `RuleId`.
// ---------------------------------------------------------------------------

/// Dispatch one rule against one pull (or the synthetic global pull),
/// honouring the LOW_RATE flag for rules that downgrade on slow logs.
pub fn dispatch(
    rule: &Rule,
    log: &ResampledLog,
    dtcs: &[String],
    pull: &Pull,
    low_rate: bool,
) -> Vec<Finding> {
    let id = match rule.id {
        "R01" => RuleId::R01, "R02" => RuleId::R02, "R03" => RuleId::R03,
        "R04" => RuleId::R04, "R05" => RuleId::R05, "R06" => RuleId::R06,
        "R07" => RuleId::R07, "R08" => RuleId::R08, "R09" => RuleId::R09,
        "R10" => RuleId::R10, "R11" => RuleId::R11, "R12" => RuleId::R12,
        "R13" => RuleId::R13, "R14" => RuleId::R14, "R15" => RuleId::R15,
        "R16" => RuleId::R16, "R17" => RuleId::R17, "R18" => RuleId::R18,
        "R19" => RuleId::R19, "R20" => RuleId::R20, "R21" => RuleId::R21,
        "R22" => RuleId::R22, "R23" => RuleId::R23,
        _ => return Vec::new(),
    };
    match id {
        RuleId::R01 => r01_underboost(log, pull),
        RuleId::R02 => r02_overboost_spike(log, pull),
        RuleId::R03 => r03_boost_target_excessive(log, pull),
        RuleId::R04 => r04_no_taper(log, pull),
        RuleId::R05 => r05_maf_below_spec(log, pull),
        RuleId::R06 => r06_lambda_floor(log, pull),
        RuleId::R07 => r07_peak_iq(log, pull),
        RuleId::R08 => r08_torque_ceiling(log, pull),
        RuleId::R09 => r09_soi_excess(log, pull, low_rate),
        RuleId::R10 => r10_eoi_late(log, pull),
        RuleId::R11 => r11_coolant_low(log, pull),
        RuleId::R12 => r12_no_atm(log, pull),
        RuleId::R13 => r13_fuel_temp(log, pull),
        RuleId::R14 => r14_srcv(log, pull),
        RuleId::R15 => r15_limp(log, pull),
        RuleId::R16 => r16_egr_observed(log, pull),
        RuleId::R17 => r17_maf_deviation(log, pull),
        RuleId::R18 => r18_cruise_nvh(log, pull),
        RuleId::R19 => r19_dtc_scan(dtcs, pull),
        RuleId::R20 => r20_maf_excess_info(log, pull),
        RuleId::R21 => r21_idle_stability(log, pull),
        RuleId::R22 => r22_low_pedal_slope(log, pull),
        RuleId::R23 => r23_coolant_trend(log, pull),
    }
}
