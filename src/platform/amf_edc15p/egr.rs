//! EGR-delete strategy module (spec §3, §9, §10 — v4).
//!
//! Encodes the canonical software EGR delete for AMF / EDC15P+:
//!
//! 1. EGR duty driven to 0 % across the entire (rpm, IQ, T_coolant, atm)
//!    domain by zeroing both banks of the EGR-duty map AND raising the
//!    spec-MAF map ≥ MAF actual at every cell. Redundant pair so the PID
//!    never asks for EGR.
//! 2. P0401–P0403 (Group A — real on AMF) DTC enable-flags suppressed,
//!    OR thresholds widened so they will not trigger. P0404–P0406 (Group
//!    B — defensive; should not appear on AMF, no EGR position sensor)
//!    suppressed as belt-and-braces.
//! 3. MAF stays the closed-loop input — the MAF/MAP smoke-limiter switch
//!    is left at `0x00` (MAF-based).
//!
//! Hardware stays in place: EGR valve, EGR cooler, vacuum lines, ASV all
//! remain installed. Software-only delete works because the EGR valve is
//! vacuum-actuated and spring-return-to-closed: with 0 % duty there is
//! no vacuum, the spring closes the valve, and no exhaust flows into the
//! intake.
//!
//! The tool does NOT generate byte patches. It emits the symbolic
//! targets ([`EGR_DUTY_MAP_NAME_BANK_A`], [`EGR_DUTY_MAP_NAME_BANK_B`],
//! [`SPEC_MAF_MAP_NAME`], the DTC list) and lets the user resolve byte
//! addresses in their tuning tool against their actual binary's DAMOS.

use crate::platform::amf_edc15p::envelope::CAPS;

/// Symbolic name of the EGR-duty map, bank A (`arwMEAB0KL` in the
/// public EDC15 DAMOS map-pack).
pub const EGR_DUTY_MAP_NAME_BANK_A: &str = "AGR_arwMEAB0KL";

/// Symbolic name of the EGR-duty map, bank B (`arwMEAB1KL`). Paired in
/// DAMOS even on single-actuator PD ECUs.
pub const EGR_DUTY_MAP_NAME_BANK_B: &str = "AGR_arwMEAB1KL";

/// Symbolic name of the spec-MAF (expected air mass) map. Re-flattened
/// to ≥ MAF actual at every cell so the EGR PID setpoint is always
/// satisfied without asking for EGR.
pub const SPEC_MAF_MAP_NAME: &str = "arwMLGRDKF";

/// **Group A** DTCs — real on AMF. Suppression / threshold widening is
/// always required after an EGR delete.
///
/// - `P0401` EGR insufficient flow (MAF deviation inferred)
/// - `P0402` EGR excessive flow (ditto)
/// - `P0403` EGR solenoid circuit (electrical — keep enabled, see
///   [`DTC_WIRING_FAULTS`])
pub const DTC_GROUP_A: &[&str] = &["P0401", "P0402", "P0403"];

/// **Group B** DTCs — should NOT appear on AMF (no EGR position sensor).
/// Suppressed as defensive belt-and-braces in case of code-list mistake
/// or non-AMF ECU file.
pub const DTC_GROUP_B: &[&str] = &["P0404", "P0405", "P0406"];

/// All DTCs the v4 EGR-delete suppression covers (Group A + Group B).
pub const DTC_LIST_TO_SUPPRESS: &[&str] = &[
    "P0401", "P0402", "P0403", "P0404", "P0405", "P0406",
];

/// DTCs that indicate a real wiring fault — these should NOT be silently
/// suppressed. R19 surfaces them with a different remediation pointer.
pub const DTC_WIRING_FAULTS: &[&str] = &["P0403"];

/// Idle-stability threshold (RPM σ over a 30-s warm-idle window), used
/// by R21 at Warn severity.
pub const IDLE_INSTABILITY_THRESHOLD_RPM_STD: f64 = 25.0;

/// Stricter Info-level idle-stability threshold (RPM σ over the same
/// window). Spec §14 open-question 4: a tightened threshold suitable for
/// known-good injectors.
pub const IDLE_INSTABILITY_INFO_RPM_STD: f64 = 15.0;

/// Minimum window length (seconds) for R21 to fire at Warn severity.
/// Below this, R21 downgrades to Info regardless of σ.
pub const IDLE_WINDOW_MIN_S: f64 = 30.0;

/// Cruise band for the SOI NVH retard recommendation (R18).
/// `(rpm_lo, rpm_hi, iq_lo_mg, iq_hi_mg)`.
pub const CRUISE_BAND: (f64, f64, f64, f64) = (1500.0, 2500.0, 5.0, 15.0);

/// Default cruise-band SOI retard (deg) recommended after EGR delete to
/// mitigate the marginal NVH increase (faster premixed phase with no
/// inert charge to slow it).
pub const CRUISE_SOI_RETARD_DEG: f64 = 1.0;

/// Strategy-B fill value for the spec-MAF map (mg/stroke). Re-exported
/// from [`CAPS`] for convenience.
pub const SPEC_MAF_FILL_MGSTR: f64 = CAPS.spec_maf_fill_mg_stroke;

/// EGR duty observed-tolerance for R16 (and the validation checklist).
/// Anything above this in any sample is taken as evidence the delete was
/// not applied.
pub const EGR_DUTY_OBSERVED_TOLERANCE_PCT: f64 = 2.0;

/// Maximum acceptable cruise warm-engine `|MAF_actual − MAF_spec| /
/// MAF_spec` deviation (R17 threshold).
pub const MAF_DEVIATION_FRACTION: f64 = 0.15;

/// Minimum sustained-deviation duration (seconds) before R17 fires.
pub const MAF_DEVIATION_MIN_DURATION_S: f64 = 2.0;

/// `MAF_actual − MAF_spec` excess (mg/stroke) above which R20 (was R17b)
/// classifies the cruise sample as "delete is functional, spec-MAF
/// intentionally saturated".
pub const MAF_EXCESS_INFO_MG: f64 = 50.0;

/// WOT pedal cutoff (%) for R17 — at and above this pedal value, MAF
/// deviation is not assessed because EGR is already off in the stock
/// calibration.
pub const WOT_PEDAL_CUTOFF_PCT: f64 = 80.0;

/// Maximum cruise pedal (%) used by R18 to gate the NVH check.
pub const CRUISE_PEDAL_MAX_PCT: f64 = 30.0;

/// Maximum idle pedal (%) used by R21 / validation checks to define an
/// "idle" sample.
pub const IDLE_PEDAL_MAX_PCT: f64 = 5.0;

/// Maximum idle IQ (mg/stroke) used as a fallback definition of "idle"
/// when a pedal channel is unavailable.
pub const IDLE_IQ_MAX_MG: f64 = 8.0;

/// One change recommended on a symbolic map. Banks always parallel —
/// the user applies the same delta to both banks of the EDC15P+ bin.
#[derive(Debug, Clone, PartialEq)]
pub struct MapDelta {
    /// Symbolic map name (matches the entries in the maps registry plus
    /// the EGR-specific [`EGR_DUTY_MAP_NAME_BANK_A`] /
    /// [`EGR_DUTY_MAP_NAME_BANK_B`] / [`SPEC_MAF_MAP_NAME`]).
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

/// The full v4 EGR-delete recommendation set, independent of any log
/// evidence. These are unconditional — the v4 mandate is that every
/// flash includes them.
pub fn recommend_egr_delete_deltas() -> Vec<MapDelta> {
    vec![
        MapDelta {
            map_name: EGR_DUTY_MAP_NAME_BANK_A.to_string(),
            cell_selector: "all cells, bank A".to_string(),
            action: "set to 0% (valve-closed-fill, polarity per loaded file)".to_string(),
            value: Some(0.0),
            rationale:
                "Primary actuator path. Zeroing the bank-A EGR-duty map drives the EGR vacuum \
                 solenoid to 0 % so the spring-return valve stays mechanically closed. \
                 Hardware stays installed; only the duty map is flashed."
                    .to_string(),
        },
        MapDelta {
            map_name: EGR_DUTY_MAP_NAME_BANK_B.to_string(),
            cell_selector: "all cells, bank B".to_string(),
            action: "set to 0% (paired with bank A)".to_string(),
            value: Some(0.0),
            rationale:
                "DAMOS lists arwMEAB1KL as a paired second-bank EGR map even on \
                 single-actuator PD ECUs. Defensive parity: write both banks identically."
                    .to_string(),
        },
        MapDelta {
            map_name: SPEC_MAF_MAP_NAME.to_string(),
            cell_selector: "all cells, both banks".to_string(),
            action: format!(
                "fill {SPEC_MAF_FILL_MGSTR} mg/stroke (Strategy B — saturated)"
            ),
            value: Some(SPEC_MAF_FILL_MGSTR),
            rationale:
                "Belt-and-braces: makes MAF_actual − MAF_spec permanently ≤ 0 so the EGR PID \
                 never asks for EGR even if the duty map is interpreted with the opposite \
                 polarity convention. 850 mg/str at 3000 rpm WOT is the canonical Bosch HFM5 \
                 calibration target on the 1.9 R4 PD family."
                    .to_string(),
        },
        MapDelta {
            map_name: "DTC_thresholds".to_string(),
            cell_selector: format!(
                "Group A: {} (always); Group B: {} (defensive)",
                DTC_GROUP_A.join(", "),
                DTC_GROUP_B.join(", "),
            ),
            action: "widen MAF-deviation thresholds and time-debounce so they cannot trip"
                .to_string(),
            value: None,
            rationale:
                "Group A (P0401/P0402/P0403) is genuinely possible on AMF: P0401/P0402 are \
                 inferred from MAF deviation; P0403 is the EGR solenoid wiring fault and \
                 should remain a real diagnostic. Group B (P0404/P0405/P0406) requires an \
                 EGR position sensor that AMF does not have — suppressed defensively."
                    .to_string(),
        },
        MapDelta {
            map_name: "MAF_MAP_smoke_switch".to_string(),
            cell_selector: "switch byte (DAMOS-located)".to_string(),
            action: "LEAVE STOCK at 0x00 (MAF-based smoke limiting)".to_string(),
            value: None,
            rationale:
                "v4 explicitly keeps MAF closed-loop. HFM5 is in spec, post-delete MAF is well \
                 within sensor linear range, and the IQ-by-MAF map is real and well-shaped on \
                 AMF — flipping to MAP would expose the engine to a flat / unused IQ-by-MAP \
                 map. See spec §3.2."
                    .to_string(),
        },
        MapDelta {
            map_name: "Idle_fuel".to_string(),
            cell_selector: format!(
                "warm idle (T_coolant ≥ {:.0}°C, IQ ≤ {IDLE_IQ_MAX_MG} mg)",
                CAPS.warm_coolant_min_c
            ),
            action: "−1.5 mg/stroke (CONDITIONAL — only if R21 idle-stability fires)".to_string(),
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
    fn dtc_groups_are_disjoint_and_complete() {
        for c in DTC_GROUP_A {
            assert!(!DTC_GROUP_B.contains(c), "{c} cannot be in both groups");
            assert!(DTC_LIST_TO_SUPPRESS.contains(c));
        }
        for c in DTC_GROUP_B {
            assert!(DTC_LIST_TO_SUPPRESS.contains(c));
        }
        assert_eq!(DTC_GROUP_A.len() + DTC_GROUP_B.len(), DTC_LIST_TO_SUPPRESS.len());
    }

    #[test]
    fn p0403_flagged_as_wiring_fault() {
        assert!(DTC_WIRING_FAULTS.contains(&"P0403"));
    }

    #[test]
    fn delete_deltas_include_both_egr_banks_and_spec_maf() {
        let deltas = recommend_egr_delete_deltas();
        assert!(deltas.iter().any(|d| d.map_name == EGR_DUTY_MAP_NAME_BANK_A));
        assert!(deltas.iter().any(|d| d.map_name == EGR_DUTY_MAP_NAME_BANK_B));
        assert!(deltas.iter().any(|d| d.map_name == SPEC_MAF_MAP_NAME));
        assert!(deltas.iter().any(|d| d.map_name == "DTC_thresholds"));
        assert!(deltas.iter().any(|d| d.map_name == "MAF_MAP_smoke_switch"));
        assert!(deltas.iter().any(|d| d.map_name == "SOI_warm_cruise"));
    }

    #[test]
    fn delete_deltas_egr_duty_is_zero_in_both_banks() {
        let deltas = recommend_egr_delete_deltas();
        let bank_a = deltas.iter().find(|d| d.map_name == EGR_DUTY_MAP_NAME_BANK_A).unwrap();
        let bank_b = deltas.iter().find(|d| d.map_name == EGR_DUTY_MAP_NAME_BANK_B).unwrap();
        assert_eq!(bank_a.value, Some(0.0));
        assert_eq!(bank_b.value, Some(0.0));
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
        assert!((p[0] - 220.0).abs() < 1e-9);
        assert!(p[1] > p[0]);
        assert!(p[2] > p[1]);
    }

    #[test]
    fn predict_maf_clamps_at_50_percent() {
        let p_high = predict_maf_no_egr(&[200.0], &[80.0]);
        let p_cap = predict_maf_no_egr(&[200.0], &[50.0]);
        assert_eq!(p_high, p_cap);
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
