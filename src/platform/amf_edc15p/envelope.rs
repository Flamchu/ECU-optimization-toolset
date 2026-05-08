//! Hard guardrails per spec §7 — the "sane Stage 1" envelope (v4).
//!
//! Every recommendation passes through one of the `clamp_*` functions
//! before reaching the report. If a delta would push the resulting value
//! outside this envelope, the engine emits `BLOCKED — envelope cap` and
//! the cap that fired is named in the rationale.
//!
//! Each cap below has a documented physical/longevity reason — never
//! raise one without updating the rationale alongside it.

/// Diesel stoichiometric AFR (kg air / kg fuel). Industry consensus: 14.5
/// (acceptable band 14.4–14.6). Used in the lambda model
/// (lambda = MAF / (IQ × 14.5)).
pub const DIESEL_AFR_STOICH: f64 = 14.5;

/// Modelled-torque conversion (re-exported from `stock_refs` for ergonomics).
/// Calibration-tuned engineering constant: 44.5 mg × 4.4 ≈ 195 Nm stock.
pub const NM_PER_MG_IQ: f64 = super::stock_refs::NM_PER_MG_IQ;

/// Numeric envelope caps. Values are absolute caps the tool must NEVER
/// exceed.
#[derive(Debug, Clone, Copy)]
pub struct EnvelopeCaps {
    /// Right edge of the GT1544S efficient compressor map (mbar absolute).
    pub peak_boost_mbar_abs: i32,
    /// Tighter cap above 4000 rpm: GT1544S compressor map narrows past
    /// ~6 lb/min @ PR > 2.0 (choke flow + shaft overspeed risk).
    pub peak_boost_above_4000rpm_mbar_abs: i32,

    /// PD75 nozzle duration headroom + LUK clutch torque ceiling.
    /// v3 raised from 52 mg/stroke (smoke removed → only injector-duty
    /// + clutch matter); v4 keeps 54 mg/str.
    pub peak_iq_mg: f64,
    /// Lambda floor for a diesel screening tool.
    /// Industry consensus: below λ ≈ 1.05 you get incomplete combustion,
    /// soot ramp, EGT spike. Unified at 1.05 (was inconsistent 1.05/1.20
    /// across v3 modules).
    pub lambda_floor: f64,
    /// Cast-iron manifold creep onset ≥750 °C; 800 °C sustained binding cap.
    /// Pistons have no oil cooling jets on AMF, which makes this load-bearing.
    pub pre_turbo_egt_c_sustained: i32,

    /// SOI advance cap (deg BTDC) at IQ ≥ `soi_iq_threshold_mg`.
    pub soi_max_btdc: f64,
    /// IQ at and above which the SOI cap kicks in (cruise / cold maps may
    /// run more advanced below this).
    pub soi_iq_threshold_mg: f64,
    /// EOI cap (deg ATDC); past this, heat dumps into the turbine.
    pub eoi_max_atdc: f64,

    /// LUK SMF clutch ceiling (Nm flywheel). Engineering judgement (LUK
    /// does not publish a torque rating for the OE diesel SMF on this
    /// platform).
    pub modelled_flywheel_torque_nm: f64,

    /// ECU map quantisation ceiling (mg/stroke). NOT a Bosch HFM5 sensor
    /// saturation point (HFM5 itself does not saturate on AMF airflow);
    /// this is the Strategy-B safe envelope on the EDC15P+ map tables.
    pub maf_max_mg_stroke: f64,

    /// Strategy-B fill value for the spec-MAF map (mg/stroke). Canonical
    /// Bosch HFM5 calibration target at 3000 rpm WOT for the 1.9 R4 PD
    /// family; AMF inherits the same target. Saturating arwMLGRDKF here
    /// guarantees `MAF_actual − MAF_spec` is never positive so the EGR
    /// PID never demands EGR.
    pub spec_maf_fill_mg_stroke: f64,

    /// SVBL change cap — never touch the overboost cut.
    pub svbl_change_mbar: i32,

    /// Maximum permitted EGR duty (%) in any recommended map. Mandatory
    /// software EGR delete: every cell must be 0 %.
    pub egr_duty_max_pct: f64,

    /// Coolant minimum (°C) for "warm pull" (R11 — invalidate the pull
    /// otherwise; cold SOI map is in play below).
    pub coolant_pull_min_c: f64,
    /// Coolant minimum (°C) for "warm cruise / warm idle" (R17 / R18 / R21).
    /// Lower than `coolant_pull_min_c` because cruise/idle screening only
    /// needs the engine off the cold-start map.
    pub warm_coolant_min_c: f64,

    /// Pedal threshold (%) for WOT pull detection. VCDS pedal channel
    /// rounds to 1 % steps; 95 % robustly excludes "near-WOT" coast-up.
    pub pedal_wot_pct: f64,
}

/// Canonical, frozen instance of [`EnvelopeCaps`] (v4 audit-reconciled
/// edition).
pub const CAPS: EnvelopeCaps = EnvelopeCaps {
    peak_boost_mbar_abs: 2150,
    peak_boost_above_4000rpm_mbar_abs: 2050,
    peak_iq_mg: 54.0,
    lambda_floor: 1.05,
    pre_turbo_egt_c_sustained: 800,
    soi_max_btdc: 26.0,
    soi_iq_threshold_mg: 30.0,
    eoi_max_atdc: 10.0,
    modelled_flywheel_torque_nm: 240.0,
    maf_max_mg_stroke: 1000.0,
    spec_maf_fill_mg_stroke: 850.0,
    svbl_change_mbar: 0,
    egr_duty_max_pct: 0.0,
    coolant_pull_min_c: 80.0,
    warm_coolant_min_c: 70.0,
    pedal_wot_pct: 95.0,
};

/// Result of running a single proposed value through the envelope.
#[derive(Debug, Clone, PartialEq)]
pub struct ClampOutcome {
    /// What the engine should actually emit: original if not blocked,
    /// the cap value if blocked.
    pub value: f64,
    /// Whether a cap fired.
    pub blocked: bool,
    /// Name of the specific guardrail that fired (empty if not blocked).
    pub cap_name: &'static str,
    /// Plain-English explanation suitable for the report rationale.
    pub explanation: String,
}

impl ClampOutcome {
    fn ok(value: f64) -> Self {
        Self { value, blocked: false, cap_name: "", explanation: String::new() }
    }

    fn blocked(cap_value: f64, cap_name: &'static str, explanation: String) -> Self {
        Self { value: cap_value, blocked: true, cap_name, explanation }
    }
}

/// Cap boost target by RPM. Above 4000 rpm uses the tighter taper cap.
pub fn clamp_boost_target(proposed_mbar_abs: f64, rpm: f64) -> ClampOutcome {
    if rpm >= 4000.0 && proposed_mbar_abs > f64::from(CAPS.peak_boost_above_4000rpm_mbar_abs) {
        return ClampOutcome::blocked(
            f64::from(CAPS.peak_boost_above_4000rpm_mbar_abs),
            "peak_boost_above_4000rpm_mbar_abs",
            format!(
                "Garrett GT1544S compressor map narrows above 4000 rpm; sustained PR > 2.0 \
                 risks shaft overspeed. Capped at {} mbar abs.",
                CAPS.peak_boost_above_4000rpm_mbar_abs
            ),
        );
    }
    if proposed_mbar_abs > f64::from(CAPS.peak_boost_mbar_abs) {
        return ClampOutcome::blocked(
            f64::from(CAPS.peak_boost_mbar_abs),
            "peak_boost_mbar_abs",
            format!(
                "Right edge of Garrett GT1544S efficient compressor map at AMF flow rates. \
                 Capped at {} mbar abs.",
                CAPS.peak_boost_mbar_abs
            ),
        );
    }
    ClampOutcome::ok(proposed_mbar_abs)
}

/// Cap requested IQ by the PD75 injector duration headroom + LUK clutch
/// ceiling.
pub fn clamp_iq(proposed_mg: f64) -> ClampOutcome {
    if proposed_mg > CAPS.peak_iq_mg {
        return ClampOutcome::blocked(
            CAPS.peak_iq_mg,
            "peak_iq_mg",
            format!(
                "PD75 nozzle duration headroom + LUK clutch torque ceiling. \
                 Capped at {} mg/stroke.",
                CAPS.peak_iq_mg
            ),
        );
    }
    ClampOutcome::ok(proposed_mg)
}

/// Cap SOI advance.
///
/// At IQ below `soi_iq_threshold_mg` (cruise / cold start) more advance is
/// thermally survivable, so SOI is returned unchanged. At and above the
/// threshold the cap of `soi_max_btdc` is enforced.
pub fn clamp_soi(proposed_btdc: f64, iq_mg: f64) -> ClampOutcome {
    if iq_mg < CAPS.soi_iq_threshold_mg {
        return ClampOutcome::ok(proposed_btdc);
    }
    if proposed_btdc > CAPS.soi_max_btdc {
        return ClampOutcome::blocked(
            CAPS.soi_max_btdc,
            "soi_max_btdc",
            format!(
                "At IQ ≥ {} mg, advance beyond {}° BTDC migrates peak cylinder \
                 pressure ahead of TDC and stresses the unjacketed pistons. \
                 Capped at {}° BTDC.",
                CAPS.soi_iq_threshold_mg, CAPS.soi_max_btdc, CAPS.soi_max_btdc
            ),
        );
    }
    ClampOutcome::ok(proposed_btdc)
}

/// Cap modelled flywheel torque at the LUK SMF clutch ceiling.
pub fn clamp_torque_nm(proposed_nm: f64) -> ClampOutcome {
    if proposed_nm > CAPS.modelled_flywheel_torque_nm {
        return ClampOutcome::blocked(
            CAPS.modelled_flywheel_torque_nm,
            "modelled_flywheel_torque_nm",
            format!(
                "LUK SMF clutch ceiling (engineering judgement: 195 Nm rated × 1.23 headroom; \
                 LUK does not publish a torque rating for this OE clutch). Capped at {} Nm.",
                CAPS.modelled_flywheel_torque_nm
            ),
        );
    }
    ClampOutcome::ok(proposed_nm)
}

/// Reject any commanded operating point below the lambda floor.
pub fn clamp_lambda_floor(proposed_lambda: f64) -> ClampOutcome {
    if proposed_lambda < CAPS.lambda_floor {
        return ClampOutcome::blocked(
            CAPS.lambda_floor,
            "lambda_floor",
            format!(
                "Below λ = {} on diesel = visible smoke + EGT spike + ring-land stress. \
                 Floor enforced.",
                CAPS.lambda_floor
            ),
        );
    }
    ClampOutcome::ok(proposed_lambda)
}

/// Refuse any non-zero change to SVBL (overboost cut).
pub fn clamp_svbl(proposed_change_mbar: f64) -> ClampOutcome {
    if proposed_change_mbar != 0.0 {
        return ClampOutcome::blocked(
            0.0,
            "svbl_change_mbar",
            "SVBL is the last line of defence against overboost; never touch.".to_string(),
        );
    }
    ClampOutcome::ok(0.0)
}

/// Refuse any non-zero EGR duty in a recommended map. The v3/v4 software
/// EGR delete is mandatory; mechanical hardware stays installed.
pub fn clamp_egr_duty_pct(proposed_duty_pct: f64) -> ClampOutcome {
    if proposed_duty_pct.abs() > f64::EPSILON {
        return ClampOutcome::blocked(
            CAPS.egr_duty_max_pct,
            "egr_duty_max_pct",
            "v4 mandates software EGR delete: EGR duty must be 0 % in every \
             recommended map cell. Mechanical EGR hardware stays installed; \
             only the duty map and spec-MAF map are flashed."
                .to_string(),
        );
    }
    ClampOutcome::ok(CAPS.egr_duty_max_pct)
}

/// Cap a spec-MAF cell so it never falls below the saturation fill value.
/// The Strategy-B default (850 mg/stroke) saturates the EGR PID setpoint
/// so it never demands EGR.
pub fn clamp_spec_maf(proposed_mg_stroke: f64) -> ClampOutcome {
    if proposed_mg_stroke < CAPS.spec_maf_fill_mg_stroke {
        return ClampOutcome::blocked(
            CAPS.spec_maf_fill_mg_stroke,
            "spec_maf_fill_mg_stroke",
            format!(
                "Spec-MAF cells must be ≥ {} mg/stroke (canonical Bosch HFM5 calibration \
                 target at 3000 rpm WOT). Saturating here makes MAF_actual − MAF_spec never \
                 positive so the EGR PID never demands EGR.",
                CAPS.spec_maf_fill_mg_stroke
            ),
        );
    }
    ClampOutcome::ok(proposed_mg_stroke)
}

/// Reject end-of-injection later than 10° ATDC.
pub fn clamp_eoi_atdc(proposed_eoi_atdc: f64) -> ClampOutcome {
    if proposed_eoi_atdc > CAPS.eoi_max_atdc {
        return ClampOutcome::blocked(
            CAPS.eoi_max_atdc,
            "eoi_max_atdc",
            format!(
                "EOI past {}° ATDC dumps unburned heat into the turbine — \
                 high EGT and poor BSFC.",
                CAPS.eoi_max_atdc
            ),
        );
    }
    ClampOutcome::ok(proposed_eoi_atdc)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boost_below_cap_passes() {
        let r = clamp_boost_target(2000.0, 3000.0);
        assert!(!r.blocked);
        assert_eq!(r.value, 2000.0);
    }

    #[test]
    fn boost_above_cap_clamped() {
        let r = clamp_boost_target(2300.0, 3000.0);
        assert!(r.blocked);
        assert_eq!(r.cap_name, "peak_boost_mbar_abs");
        assert_eq!(r.value, 2150.0);
    }

    #[test]
    fn boost_above_4k_uses_tighter_cap() {
        let r = clamp_boost_target(2100.0, 4500.0);
        assert!(r.blocked);
        assert_eq!(r.cap_name, "peak_boost_above_4000rpm_mbar_abs");
        assert_eq!(r.value, 2050.0);
    }

    #[test]
    fn iq_at_cap_passes() {
        let r = clamp_iq(54.0);
        assert!(!r.blocked);
    }

    #[test]
    fn iq_above_cap_blocked() {
        let r = clamp_iq(56.0);
        assert!(r.blocked);
        assert_eq!(r.value, 54.0);
    }

    #[test]
    fn iq_v2_old_cap_now_passes() {
        let r = clamp_iq(52.0);
        assert!(!r.blocked);
    }

    #[test]
    fn soi_above_cap_blocked_at_high_iq() {
        let r = clamp_soi(28.0, 45.0);
        assert!(r.blocked);
        assert_eq!(r.value, 26.0);
    }

    #[test]
    fn soi_below_threshold_iq_returns_unchanged() {
        // v4 fix I: at IQ < 30 mg, SOI cap does not apply (cruise / cold).
        let r = clamp_soi(28.0, 10.0);
        assert!(!r.blocked);
        assert_eq!(r.value, 28.0);
    }

    #[test]
    fn soi_at_threshold_iq_clamps() {
        // Boundary: iq_mg == 30.0 must enforce the cap.
        let r = clamp_soi(28.0, 30.0);
        assert!(r.blocked);
        assert_eq!(r.value, CAPS.soi_max_btdc);
    }

    #[test]
    fn torque_above_cap_blocked() {
        let r = clamp_torque_nm(260.0);
        assert!(r.blocked);
        assert_eq!(r.value, 240.0);
    }

    #[test]
    fn lambda_below_floor_blocked() {
        let r = clamp_lambda_floor(1.00);
        assert!(r.blocked);
    }

    #[test]
    fn lambda_at_v4_floor_passes() {
        let r = clamp_lambda_floor(1.05);
        assert!(!r.blocked);
    }

    #[test]
    fn lambda_at_v2_old_floor_now_passes() {
        // v2's 1.20 used to be the floor; v3+v4 keep 1.05.
        let r = clamp_lambda_floor(1.10);
        assert!(!r.blocked);
    }

    #[test]
    fn svbl_change_blocked() {
        let r = clamp_svbl(50.0);
        assert!(r.blocked);
        assert_eq!(r.value, 0.0);
    }

    #[test]
    fn eoi_above_cap_blocked() {
        let r = clamp_eoi_atdc(15.0);
        assert!(r.blocked);
        assert_eq!(r.value, 10.0);
    }

    #[test]
    fn egr_duty_zero_passes() {
        let r = clamp_egr_duty_pct(0.0);
        assert!(!r.blocked);
        assert_eq!(r.value, 0.0);
    }

    #[test]
    fn egr_duty_nonzero_blocked() {
        let r = clamp_egr_duty_pct(15.0);
        assert!(r.blocked);
        assert_eq!(r.value, 0.0);
        assert_eq!(r.cap_name, "egr_duty_max_pct");
    }

    #[test]
    fn spec_maf_at_fill_passes() {
        let r = clamp_spec_maf(850.0);
        assert!(!r.blocked);
    }

    #[test]
    fn spec_maf_below_fill_blocked() {
        let r = clamp_spec_maf(400.0);
        assert!(r.blocked);
        assert_eq!(r.value, 850.0);
    }

    #[test]
    fn caps_lambda_floor_is_v4_canonical() {
        // v4 acceptance #4: λ floor unified at 1.05 across the codebase.
        assert!((CAPS.lambda_floor - 1.05).abs() < f64::EPSILON);
    }

    #[test]
    fn caps_coolant_constants_disambiguated() {
        // v4 fix F + Y: pull-coolant minimum (80) and warm-cruise (70) split.
        assert!((CAPS.coolant_pull_min_c - 80.0).abs() < f64::EPSILON);
        assert!((CAPS.warm_coolant_min_c - 70.0).abs() < f64::EPSILON);
        // The relative ordering is also a compile-time invariant — see
        // `_SELF_CONSISTENT` in tests/integration_envelope.rs.
    }
}
