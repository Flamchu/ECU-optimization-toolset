//! Default sane Stage 1 deltas per spec §9 (v4).
//!
//! Used by the recommendation engine when no user-supplied basemap is
//! available; each delta is then run through `envelope::clamp_*` to
//! confirm the resulting value stays inside the envelope.

use crate::platform::amf_edc15p::envelope::CAPS;

/// Kind of action a [`DefaultDelta`] represents.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeltaKind {
    /// Add a delta in mbar to the current value.
    DeltaMbar,
    /// Add a delta in mg/stroke.
    DeltaMg,
    /// Add a delta in degrees.
    DeltaDeg,
    /// Set the value to a fixed target.
    SetTo,
    /// Leave the map at stock.
    LeaveStock,
    /// Extend the axis end to a new endpoint.
    ExtendAxis,
    /// Clamp the peak of the surface to a fixed value (or via lambda model).
    ClampPeak,
    /// Zero an EGR-related map (force every cell to 0 % duty).
    ZeroEgr,
    /// Fill a spec-MAF cell range to the Strategy-B saturation value.
    FillSpecMaf,
    /// Widen / disable a DTC plausibility threshold so it cannot trip
    /// after the EGR delete.
    SuppressDtc,
    /// Flatten a map region to a target maximum dY/dX slope. Used by the
    /// Driver_Wish low-pedal row: the operator targets the slope, the
    /// tool does not rewrite cells. `value` carries the target slope
    /// (mg/stroke per percent of pedal).
    Flatten,
    /// Set the four cooling-fan stage thresholds (stage-1 on/off, stage-2
    /// on/off). The four numeric values are derived from CAPS at
    /// recommendation-time and rendered in the action string after each
    /// is run through `clamp_fan_on_c`.
    FanThresholds,
    /// Add a positive delta (in seconds) to the fan run-on duration.
    /// `value` carries the delta; the result is capped via
    /// `clamp_fan_run_on_s` against the absolute total ceiling.
    FanRunOn,
}

/// One row in the default sane-deltas table.
#[derive(Debug, Clone, Copy)]
pub struct DefaultDelta {
    /// Symbolic map this delta targets.
    pub map_name: &'static str,
    /// Free-text cell selector (e.g. `"rpm 2000-3500 × IQ 40-50 mg"`).
    pub cell_selector: &'static str,
    /// Action kind.
    pub kind: DeltaKind,
    /// Delta / target / extend-to value, where applicable.
    pub value: Option<f64>,
    /// Rule ids that need to fire to APPLY this delta.
    pub rule_refs: &'static [&'static str],
    /// Note printed verbatim in the report rationale.
    pub note: &'static str,
}

/// Canonical default-delta table (v4, 22 rows — added bank-B EGR map).
pub const DEFAULT_DELTAS: &[DefaultDelta] = &[
    DefaultDelta {
        map_name: "LDRXN",
        cell_selector: "rpm 2000-3500 × IQ 40-50 mg",
        kind: DeltaKind::DeltaMbar,
        value: Some(150.0),
        rule_refs: &["R02", "R03"],
        note: "Bounded to absolute ≤ 2150 mbar by clamp_boost_target.",
    },
    DefaultDelta {
        map_name: "LDRXN",
        cell_selector: "rpm 4000-4500 (taper)",
        kind: DeltaKind::DeltaMbar,
        value: Some(-50.0),
        rule_refs: &["R04"],
        note: "Hold at stock − 50 mbar to keep GT1544S in efficiency island.",
    },
    DefaultDelta {
        map_name: "Driver_Wish",
        cell_selector: "pedal 100% × rpm 1800-3500",
        kind: DeltaKind::SetTo,
        value: Some(50.0),
        rule_refs: &["R07"],
        note: "Raise to 50 mg/stroke; bounded by clamp_iq.",
    },
    DefaultDelta {
        map_name: "Smoke_IQ_by_MAP",
        cell_selector: "boost 2000-2150 mbar × rpm 2000-3500",
        kind: DeltaKind::ClampPeak,
        value: None,
        rule_refs: &["R06"],
        note: "Enforce λ ≥ 1.05 by computed IQ cap from MAF model.",
    },
    DefaultDelta {
        map_name: "Smoke_IQ_by_MAF",
        cell_selector: "MAF 600-750 mg/str × rpm 2000-3500",
        kind: DeltaKind::ClampPeak,
        value: None,
        rule_refs: &["R06"],
        note: "Same λ ≥ 1.05 floor, in MAF space.",
    },
    DefaultDelta {
        map_name: "Torque_Limiter",
        cell_selector: "full surface",
        kind: DeltaKind::ClampPeak,
        value: Some(240.0),
        rule_refs: &["R08"],
        note: "Clamp peak modelled torque to 240 Nm — LUK SMF protection (engineering judgement).",
    },
    DefaultDelta {
        map_name: "SOI",
        cell_selector: "rpm 3500-4500 × IQ 40-50 mg (warm map only)",
        kind: DeltaKind::DeltaDeg,
        value: Some(1.5),
        rule_refs: &["R09"],
        note: "+1.5° BTDC, capped at 26° absolute by clamp_soi (only at IQ ≥ 30 mg).",
    },
    DefaultDelta {
        map_name: "Duration",
        cell_selector: "X-axis end",
        kind: DeltaKind::ExtendAxis,
        value: Some(54.0),
        rule_refs: &["R07"],
        note: "Extend axis 50 → 54 mg, proportional only — matches the envelope IQ cap.",
    },
    DefaultDelta {
        map_name: "SVBL",
        cell_selector: "scalar",
        kind: DeltaKind::LeaveStock,
        value: None,
        rule_refs: &[],
        note: "Last line of defence — never touch.",
    },
    DefaultDelta {
        map_name: "N75_duty",
        cell_selector: "full surface",
        kind: DeltaKind::LeaveStock,
        value: None,
        rule_refs: &["R01"],
        note: "Leave stock unless R01 fires repeatedly.",
    },
    DefaultDelta {
        map_name: "Pilot",
        cell_selector: "full surface",
        kind: DeltaKind::LeaveStock,
        value: None,
        rule_refs: &[],
        note: "Pilot is NVH, not power. Leave alone.",
    },
    DefaultDelta {
        map_name: "MLHFM",
        cell_selector: "full surface",
        kind: DeltaKind::LeaveStock,
        value: None,
        rule_refs: &["R05"],
        note: "Leave stock unless MAF replaced or drift > 10 %.",
    },
    // ---- v4 EGR-delete additions ----------------------------------------
    DefaultDelta {
        map_name: "AGR_arwMEAB0KL",
        cell_selector: "all cells, bank A",
        kind: DeltaKind::ZeroEgr,
        value: Some(0.0),
        rule_refs: &[],
        note: "v4 mandate: software EGR delete. Bank A duty = 0 % across the entire \
               (rpm, IQ, T_coolant, atm) domain. Hardware stays installed.",
    },
    DefaultDelta {
        map_name: "AGR_arwMEAB1KL",
        cell_selector: "all cells, bank B",
        kind: DeltaKind::ZeroEgr,
        value: Some(0.0),
        rule_refs: &[],
        note: "v4 mandate: bank B duty = 0 % (paired in DAMOS even on single-actuator \
               PD ECUs — write both banks).",
    },
    DefaultDelta {
        map_name: "arwMLGRDKF",
        cell_selector: "all cells, both banks",
        kind: DeltaKind::FillSpecMaf,
        value: Some(850.0),
        rule_refs: &[],
        note: "v4 mandate: spec-MAF saturated at 850 mg/stroke (Strategy B — canonical \
               Bosch HFM5 calibration target). Belt-and-braces with the EGR-duty zero \
               so the PID never demands EGR.",
    },
    DefaultDelta {
        map_name: "DTC_thresholds",
        cell_selector: "Group A: P0401, P0402, P0403 (always); Group B: P0404, P0405, P0406 (defensive)",
        kind: DeltaKind::SuppressDtc,
        value: None,
        rule_refs: &[],
        note: "Group A (always real on AMF): widen MAF-deviation thresholds and time-debounce so \
               P0401/P0402 cannot trip after the delete; P0403 remains as a real wiring-fault \
               detector. Group B (defensive — should not appear on AMF, no EGR position sensor): \
               suppress as belt-and-braces in case of code-list mistake or non-AMF ECU.",
    },
    DefaultDelta {
        map_name: "MAF_MAP_smoke_switch",
        cell_selector: "switch byte (DAMOS-located, file-version-specific)",
        kind: DeltaKind::LeaveStock,
        value: None,
        rule_refs: &[],
        note: "v4 explicitly keeps MAF closed-loop (see spec §3.2). Switch stays at 0x00 — \
               flipping to 0x101 (MAP) would degrade part-throttle smoke control.",
    },
    DefaultDelta {
        map_name: "Idle_fuel",
        cell_selector: "warm idle (T_coolant ≥ 70°C, IQ ≤ 8 mg)",
        kind: DeltaKind::DeltaMg,
        value: Some(-1.5),
        rule_refs: &["R21"],
        note: "Conditional: −1.5 mg/stroke at idle ONLY if R21 fires (RPM σ > 25 over 30-s warm-idle window).",
    },
    DefaultDelta {
        map_name: "SOI_warm_cruise",
        cell_selector: "1500-2500 rpm × 5-15 mg, SOI maps 0..3 (warm)",
        kind: DeltaKind::DeltaDeg,
        value: Some(-1.0),
        rule_refs: &["R18"],
        note: "Cruise-band NVH retard (−1.0° BTDC). EGR-off has a faster premixed phase. \
               Cold-start SOI maps 4..9 untouched.",
    },
    DefaultDelta {
        map_name: "Lambda_limiter",
        cell_selector: "full surface",
        kind: DeltaKind::LeaveStock,
        value: None,
        rule_refs: &["R06"],
        note: "Already covered by Smoke_IQ_by_MAF/MAP rows.",
    },
    DefaultDelta {
        map_name: "Atmospheric_correction",
        cell_selector: "full surface",
        kind: DeltaKind::LeaveStock,
        value: None,
        rule_refs: &["R12"],
        note: "Don't touch unless altitude testbed.",
    },
    DefaultDelta {
        map_name: "EGT_model",
        cell_selector: "full surface",
        kind: DeltaKind::LeaveStock,
        value: None,
        rule_refs: &[],
        note: "Modelled, no measured EGT to recalibrate against.",
    },
    // ---- Driveability + thermal additions -------------------------------
    DefaultDelta {
        map_name: "Driver_Wish_low_pedal",
        cell_selector: "pedal 1-25 % (idle creep ≤ 5 % preserved)",
        kind: DeltaKind::Flatten,
        value: Some(0.40),
        rule_refs: &["R22"],
        note: "Conditional: flatten the off-idle slope so dIQ/dpedal ≤ 0.40 mg \
               per percent across the 5..25 % band. Idle-creep cells (≤ 5 %) and \
               mid-pedal cells (>25 %) are NOT touched — the high-pedal \
               Driver_Wish row stays exactly as is. Apply to all parallel banks.",
    },
    DefaultDelta {
        map_name: "Fan_thresholds",
        cell_selector: "stage-1 on/off, stage-2 on/off",
        kind: DeltaKind::FanThresholds,
        value: None,
        rule_refs: &[],
        note: "Lower stage-1 fan-on by ~2-5 °C and stage-2 by ~2 °C versus stock \
               (95-100 / ~102 °C convention). Hysteresis preserved at ≥ 5 °C; \
               stages stay at least 4 °C apart; never below the thermostat. \
               Longevity-positive after EGR delete (cast-iron manifold soaks heat \
               with no recirc cooling). All values capped via clamp_fan_on_c. \
               NOTE: on the A/C-equipped 6Y2 the 1H0959481B thermoswitch \
               (95-on/84-off low + 102-on/91-off high) remains hard-wired in \
               parallel with the J293 fan control module — the ECU CAN command \
               earlier turn-on but CANNOT raise turn-on above the thermoswitch \
               closure points.",
    },
    DefaultDelta {
        map_name: "Fan_run_on",
        cell_selector: "scalar (or LUT indexed by T_coolant at key-off)",
        kind: DeltaKind::FanRunOn,
        value: Some(60.0),
        rule_refs: &[],
        note: "+60 s post-key-off run-on, capped at the absolute battery-protective \
               ceiling via clamp_fan_run_on_s. Heat-soak after a sustained pull \
               raises under-bonnet temps for ~2 min; extending run-on keeps the \
               intercooler core and exhaust manifold cooling.",
    },
];

/// Static guarantee: the lambda value referenced by the smoke-IQ rows is
/// the same value baked into the envelope. Stops drift at compile time.
const _LAMBDA_FLOOR_CROSS_LINK: () = {
    assert!(CAPS.lambda_floor == 1.05);
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_default_has_a_known_map() {
        use crate::platform::amf_edc15p::maps::get_map;
        for d in DEFAULT_DELTAS {
            assert!(get_map(d.map_name).is_some(),
                "default delta references unknown map: {}", d.map_name);
        }
    }

    #[test]
    fn registry_has_25_default_deltas() {
        // 22 platform/EGR rows + 3 driveability/thermal rows.
        assert_eq!(DEFAULT_DELTAS.len(), 25);
    }

    #[test]
    fn both_egr_bank_rows_present() {
        let names: Vec<&str> = DEFAULT_DELTAS.iter().map(|d| d.map_name).collect();
        assert!(names.contains(&"AGR_arwMEAB0KL"));
        assert!(names.contains(&"AGR_arwMEAB1KL"));
    }

    #[test]
    fn idle_fuel_refs_idle_stability_rule() {
        let idle = DEFAULT_DELTAS.iter().find(|d| d.map_name == "Idle_fuel").unwrap();
        assert_eq!(idle.rule_refs, &["R21"]);
    }

    #[test]
    fn duration_axis_extends_to_envelope_iq_cap() {
        let dur = DEFAULT_DELTAS.iter().find(|d| d.map_name == "Duration").unwrap();
        assert_eq!(dur.value, Some(CAPS.peak_iq_mg));
    }

    #[test]
    fn smoke_rows_reference_lambda_floor_text_consistently() {
        for row in DEFAULT_DELTAS {
            if row.map_name.starts_with("Smoke_IQ") {
                assert!(row.note.contains("1.05"),
                    "smoke row {} must reference λ ≥ 1.05, got: {}", row.map_name, row.note);
                assert!(!row.note.contains("1.20"),
                    "smoke row {} must NOT reference λ ≥ 1.20", row.map_name);
            }
        }
    }

    #[test]
    fn driveability_and_thermal_rows_present() {
        let names: Vec<&str> = DEFAULT_DELTAS.iter().map(|d| d.map_name).collect();
        assert!(names.contains(&"Driver_Wish_low_pedal"));
        assert!(names.contains(&"Fan_thresholds"));
        assert!(names.contains(&"Fan_run_on"));
    }

    #[test]
    fn high_pedal_driver_wish_row_is_byte_identical() {
        // Snapshot pin: the existing 100 % × 1800-3500 row that sets WOT IQ
        // to 50 mg/stroke MUST stay exactly as it was. The new low-pedal row
        // touches a disjoint band and must NOT cross-contaminate the WOT cell.
        let dw = DEFAULT_DELTAS.iter()
            .find(|d| d.map_name == "Driver_Wish")
            .expect("Driver_Wish (high-pedal) row must exist");
        assert_eq!(dw.cell_selector, "pedal 100% × rpm 1800-3500");
        assert_eq!(dw.kind, DeltaKind::SetTo);
        assert_eq!(dw.value, Some(50.0));
        assert_eq!(dw.rule_refs, &["R07"]);
    }

    #[test]
    fn low_pedal_row_targets_correct_band() {
        let lp = DEFAULT_DELTAS.iter()
            .find(|d| d.map_name == "Driver_Wish_low_pedal")
            .expect("Driver_Wish_low_pedal row must exist");
        assert_eq!(lp.kind, DeltaKind::Flatten);
        assert_eq!(lp.value, Some(CAPS.low_pedal_slope_max_mg_per_pct));
        assert_eq!(lp.rule_refs, &["R22"]);
        // Must operate on the 1..25 % band, never on the WOT band.
        assert!(lp.cell_selector.contains("1-25"),
            "low-pedal row must restrict to 1..25 % band, got: {}", lp.cell_selector);
    }

    #[test]
    fn fan_rows_have_no_rule_gate() {
        // Fan deltas are unconditional (longevity-positive, no power impact).
        let fan_t = DEFAULT_DELTAS.iter().find(|d| d.map_name == "Fan_thresholds").unwrap();
        let fan_r = DEFAULT_DELTAS.iter().find(|d| d.map_name == "Fan_run_on").unwrap();
        assert!(fan_t.rule_refs.is_empty(), "Fan_thresholds is unconditional");
        assert!(fan_r.rule_refs.is_empty(), "Fan_run_on is unconditional");
    }
}
