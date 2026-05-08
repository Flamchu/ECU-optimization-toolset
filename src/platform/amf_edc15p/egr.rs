//! v3 EGR-delete strategy module (spec §3 + §7).
//!
//! Encodes the canonical software EGR delete for AMF / EDC15P+:
//!
//! 1. EGR duty driven to 0 % across the entire (rpm, IQ, T_coolant, atm)
//!    domain by zeroing the EGR-duty map AND raising the spec-MAF map
//!    ≥ MAF actual at every cell. Redundant pair so the PID never asks
//!    for EGR.
//! 2. P0401–P0406 DTC enable-flags suppressed, OR thresholds widened so
//!    they will not trigger.
//! 3. MAF stays the closed-loop input — the MAF/MAP smoke-limiter switch
//!    at `0x51C30/0x71C30` is left at `0x00` (MAF-based).
//!
//! Hardware stays in place: EGR valve, EGR cooler, vacuum lines, ASV all
//! remain installed. Software-only delete works because the EGR valve is
//! vacuum-actuated and spring-return-to-closed: with 0 % duty there is
//! no vacuum, the spring closes the valve, and no exhaust flows into the
//! intake.
//!
//! The tool does NOT generate byte patches. It emits the symbolic
//! targets ([`EGR_DUTY_MAP_NAME`], [`SPEC_MAF_MAP_NAME`], the DTC list)
//! and lets the user resolve byte addresses in their tuning tool.

use crate::platform::amf_edc15p::envelope::CAPS;

/// Symbolic name of the EGR-duty map (commonly known as "AGR", named in
/// WinOLS commentary as `arwMEAB0KL` / `arwMEAB1KL` on PD-family files).
pub const EGR_DUTY_MAP_NAME: &str = "AGR_arwMEAB0KL";

/// Symbolic name of the spec-MAF (expected air mass) map. Re-flattened
/// to ≥ MAF actual at every cell so the EGR PID setpoint is always
/// satisfied without asking for EGR.
pub const SPEC_MAF_MAP_NAME: &str = "arwMLGRDKF";

/// Diagnostic Trouble Codes that v3 suppresses or widens after the
/// software EGR delete.
pub const DTC_LIST_TO_SUPPRESS: &[&str] = &[
    "P0401", // EGR insufficient flow
    "P0402", // EGR excessive flow
    "P0403", // EGR solenoid circuit
    "P0404", // EGR range / performance
    "P0405", // EGR position sensor low
    "P0406", // EGR position sensor high
];

/// DTCs that indicate a real wiring fault (not just a "delete needs
/// suppressing" condition). Surfaced separately by R19.
pub const DTC_WIRING_FAULTS: &[&str] = &["P0403"];

/// Idle stability threshold (RPM standard deviation) used by R12.
pub const IDLE_INSTABILITY_THRESHOLD_RPM_STD: f64 = 25.0;

/// Cruise band for the SOI NVH retard recommendation (R18).
/// `(rpm_lo, rpm_hi, iq_lo_mg, iq_hi_mg)`.
pub const CRUISE_BAND: (f64, f64, f64, f64) = (1500.0, 2500.0, 5.0, 15.0);

/// Default cruise-band SOI retard (deg) recommended after EGR delete to
/// mitigate the marginal NVH increase (faster premixed phase with no
/// inert charge to slow it).
pub const CRUISE_SOI_RETARD_DEG: f64 = 1.0;

/// Strategy-B fill value for the spec-MAF map (mg/stroke) — the default
/// when no pre-flash log is available. Re-exported from [`CAPS`] for
/// convenience.
pub const SPEC_MAF_FILL_MGSTR: f64 = CAPS.spec_maf_fill_mg_stroke;

/// EGR duty observed-tolerance for finding R16. Anything above this in
/// any sample is taken as evidence the delete was not applied.
pub const EGR_DUTY_OBSERVED_TOLERANCE_PCT: f64 = 2.0;

/// Maximum acceptable cruise warm-engine `|MAF_actual − MAF_spec| /
/// MAF_spec` deviation (R17 threshold).
pub const MAF_DEVIATION_FRACTION: f64 = 0.15;

/// Minimum sustained-deviation duration (seconds) before R17 fires.
pub const MAF_DEVIATION_MIN_DURATION_S: f64 = 2.0;

/// MAF-actual − MAF-spec excess (mg/stroke) above which R17b classifies
/// the cruise sample as "delete is functional, spec-MAF intentionally
/// saturated".
pub const MAF_EXCESS_INFO_MG: f64 = 50.0;

/// Cold-start coolant cutoff (°C) for R17 — below this, MAF deviation is
/// not assessed because cold maps are in play.
pub const COLD_START_COOLANT_CUTOFF_C: f64 = 70.0;

/// WOT pedal cutoff (%) for R17 — at and above this pedal value, MAF
/// deviation is not assessed because EGR is already off in the stock
/// calibration.
pub const WOT_PEDAL_CUTOFF_PCT: f64 = 80.0;

/// Warm-engine coolant minimum (°C) used by R12 (idle stability) and the
/// validation checklist's idle / cruise items.
pub const WARM_COOLANT_MIN_C: f64 = 80.0;

/// Maximum idle IQ (mg/stroke) used to define an "idle" sample.
pub const IDLE_IQ_MAX_MG: f64 = 8.0;

/// Maximum cruise pedal (%) used by R18 to gate the NVH check.
pub const CRUISE_PEDAL_MAX_PCT: f64 = 30.0;

/// One change recommended on a symbolic map. Banks always parallel —
/// the user applies the same delta to both banks of the EDC15P+ bin.
#[derive(Debug, Clone, PartialEq)]
pub struct MapDelta {
    /// Symbolic map name (matches the entries in the maps registry plus
    /// the EGR-specific [`EGR_DUTY_MAP_NAME`] / [`SPEC_MAF_MAP_NAME`]).
    pub map_name: String,
    /// Cells / region the delta applies to (free text).
    pub cell_selector: String,
    /// Action description (e.g. `"set to 0%"`, `"fill 850 mg/stroke"`).
    pub action: String,
    /// Optional numeric value associated with the action.
    pub value: Option<f64>,
    /// One-paragraph rationale shown in the report.
    pub rationale: String,
}

/// The full v3 EGR-delete recommendation set, independent of any log
/// evidence. These are unconditional — the v3 mandate is that every
/// flash includes them.
pub fn recommend_egr_delete_deltas() -> Vec<MapDelta> {
    vec![
        MapDelta {
            map_name: EGR_DUTY_MAP_NAME.to_string(),
            cell_selector: "all cells, both banks".to_string(),
            action: "set to 0% (valve-closed-fill, polarity per loaded file)".to_string(),
            value: Some(0.0),
            rationale:
                "Primary actuator path. Zeroing the EGR-duty map drives the EGR vacuum solenoid \
                 to 0% so the spring-return valve stays mechanically closed. \
                 Hardware stays installed; only the duty map is flashed."
                    .to_string(),
        },
        MapDelta {
            map_name: SPEC_MAF_MAP_NAME.to_string(),
            cell_selector: "all cells, both banks".to_string(),
            action: format!(
                "fill {} mg/stroke (Strategy B — saturated)",
                SPEC_MAF_FILL_MGSTR
            ),
            value: Some(SPEC_MAF_FILL_MGSTR),
            rationale:
                "Belt-and-braces: makes MAF_actual − MAF_spec permanently ≤ 0 so the EGR PID \
                 never asks for EGR even if the duty map is interpreted with the opposite \
                 polarity convention."
                    .to_string(),
        },
        MapDelta {
            map_name: "DTC_thresholds".to_string(),
            cell_selector: format!("{} entries", DTC_LIST_TO_SUPPRESS.join(", ")),
            action: "widen MAF-deviation threshold and time-debounce so they cannot trip"
                .to_string(),
            value: None,
            rationale:
                "Threshold widening (preferred for safety): the DTC code path still runs but \
                 cannot trip on a deleted EGR. P0403 will still detect a real solenoid wiring \
                 fault."
                    .to_string(),
        },
        MapDelta {
            map_name: "MAF_MAP_smoke_switch".to_string(),
            cell_selector: "0x51C30 / 0x71C30".to_string(),
            action: "LEAVE STOCK at 0x00 (MAF-based smoke limiting)".to_string(),
            value: None,
            rationale:
                "v3 explicitly keeps MAF closed-loop. HFM5 is in-spec, post-delete MAF is well \
                 within sensor linear range, and the IQ-by-MAF map is real and well-shaped on \
                 AMF — flipping to MAP would expose the engine to a flat / unused IQ-by-MAP \
                 map. See spec §3.2."
                    .to_string(),
        },
        MapDelta {
            map_name: "Idle_fuel".to_string(),
            cell_selector: format!(
                "warm idle (T_coolant ≥ {WARM_COOLANT_MIN_C}°C, IQ ≤ {IDLE_IQ_MAX_MG} mg)"
            ),
            action: "−1.5 mg/stroke (CONDITIONAL — only if R12 idle-stability fires)".to_string(),
            value: Some(-1.5),
            rationale:
                "Post-delete idle leans out (more O₂, less inert charge). Some EA188 PD engines \
                 show a marginal NVH bump. Apply only when the log shows RPM σ > 25 over a 30 s \
                 warm-idle window."
                    .to_string(),
        },
        MapDelta {
            map_name: "SOI_warm_cruise".to_string(),
            cell_selector: format!(
                "{:.0}-{:.0} rpm × {:.0}-{:.0} mg, warm SOI maps 0..3",
                CRUISE_BAND.0, CRUISE_BAND.1, CRUISE_BAND.2, CRUISE_BAND.3
            ),
            action: format!("−{CRUISE_SOI_RETARD_DEG}° BTDC (NVH mitigation)"),
            value: Some(-CRUISE_SOI_RETARD_DEG),
            rationale:
                "EGR-off cruise has a faster premixed phase. Retarding SOI 1° in the cruise \
                 band brings combustion noise back to the stock subjective baseline. Cold-start \
                 SOI maps (4..9) untouched — cold-start integrity matters more than cruise NVH."
                    .to_string(),
        },
    ]
}

/// Strategy-A predictor: from a stock (pre-delete) log, estimate what
/// MAF actual would read with EGR off. Uses the observed EGR fraction
/// in each sample (`maf_actual / maf_spec` is *not* the EGR fraction —
/// the EGR fraction comes from the spec-MAF / no-EGR-MAF model the ECU
/// uses internally; we approximate from the egr_duty signal).
///
/// The simple, defensible model: assume MAF actual scales as
/// `1 / (1 − f)` with `f = clamp(egr_duty_pct / 100, 0, 0.5)`. At idle
/// EGR is bounded around 30–40 %; we cap at 50 % so the predictor never
/// extrapolates into nonsense. Returns one predicted value per input
/// sample, with NaN where any input is non-finite.
pub fn predict_maf_no_egr(maf_actual_mg: &[f64], egr_duty_pct: &[f64]) -> Vec<f64> {
    debug_assert_eq!(maf_actual_mg.len(), egr_duty_pct.len());
    let max_egr_fraction = 0.5;
    maf_actual_mg
        .iter()
        .zip(egr_duty_pct.iter())
        .map(|(&maf, &duty)| {
            if !maf.is_finite() || !duty.is_finite() {
                return f64::NAN;
            }
            let f = (duty / 100.0).clamp(0.0, max_egr_fraction);
            // (1 - f) cannot be ≤ 0 because f ≤ 0.5.
            maf / (1.0 - f) * 1.10 // 10% margin per spec §3.3
        })
        .collect()
}

/// Whether `cruise_band` contains the (rpm, iq) point.
pub fn in_cruise_band(rpm: f64, iq_mg: f64) -> bool {
    let (rpm_lo, rpm_hi, iq_lo, iq_hi) = CRUISE_BAND;
    rpm >= rpm_lo && rpm <= rpm_hi && iq_mg >= iq_lo && iq_mg <= iq_hi
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dtc_list_starts_with_p0401() {
        assert_eq!(DTC_LIST_TO_SUPPRESS[0], "P0401");
    }

    #[test]
    fn p0403_flagged_as_wiring_fault() {
        assert!(DTC_WIRING_FAULTS.contains(&"P0403"));
    }

    #[test]
    fn delete_deltas_include_egr_and_spec_maf() {
        let deltas = recommend_egr_delete_deltas();
        assert!(deltas.iter().any(|d| d.map_name == EGR_DUTY_MAP_NAME));
        assert!(deltas.iter().any(|d| d.map_name == SPEC_MAF_MAP_NAME));
        assert!(deltas.iter().any(|d| d.map_name == "DTC_thresholds"));
        assert!(deltas.iter().any(|d| d.map_name == "MAF_MAP_smoke_switch"));
        assert!(deltas.iter().any(|d| d.map_name == "SOI_warm_cruise"));
    }

    #[test]
    fn delete_deltas_egr_duty_is_zero() {
        let deltas = recommend_egr_delete_deltas();
        let egr = deltas.iter().find(|d| d.map_name == EGR_DUTY_MAP_NAME).unwrap();
        assert_eq!(egr.value, Some(0.0));
    }

    #[test]
    fn delete_deltas_spec_maf_at_strategy_b_fill() {
        let deltas = recommend_egr_delete_deltas();
        let spec = deltas.iter().find(|d| d.map_name == SPEC_MAF_MAP_NAME).unwrap();
        assert_eq!(spec.value, Some(SPEC_MAF_FILL_MGSTR));
    }

    #[test]
    fn predict_maf_lifts_with_egr_fraction() {
        let maf = vec![200.0, 200.0, 200.0];
        let duty = vec![0.0, 30.0, 50.0];
        let p = predict_maf_no_egr(&maf, &duty);
        assert!((p[0] - 220.0).abs() < 1e-9);          // no EGR → 200 × 1.10
        assert!(p[1] > p[0], "30% duty must predict more lift than 0%");
        assert!(p[2] > p[1], "50% must predict more than 30%");
    }

    #[test]
    fn predict_maf_clamps_at_50_percent() {
        let p_high = predict_maf_no_egr(&[200.0], &[80.0]);
        let p_cap = predict_maf_no_egr(&[200.0], &[50.0]);
        assert_eq!(p_high, p_cap, "EGR fraction is clamped at 50%");
    }

    #[test]
    fn predict_maf_propagates_nan() {
        let p = predict_maf_no_egr(&[f64::NAN, 200.0], &[10.0, f64::NAN]);
        assert!(p[0].is_nan());
        assert!(p[1].is_nan());
    }

    #[test]
    fn cruise_band_includes_centre() {
        assert!(in_cruise_band(2000.0, 10.0));
    }

    #[test]
    fn cruise_band_excludes_high_rpm() {
        assert!(!in_cruise_band(3500.0, 10.0));
    }

    #[test]
    fn cruise_band_excludes_high_iq() {
        assert!(!in_cruise_band(2000.0, 30.0));
    }
}
