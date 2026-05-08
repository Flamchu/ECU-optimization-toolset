//! Hard guardrails per spec §5 — the "sane Stage 1" envelope.
//!
//! Every recommendation passes through one of the `clamp_*` functions
//! before reaching the report. If a delta would push the resulting value
//! outside this envelope, the engine emits `BLOCKED — envelope cap` and
//! the cap that fired is named in the rationale.
//!
//! Each cap below has a documented physical/longevity reason — never
//! raise one without updating the rationale alongside it.

/// Diesel stoichiometric AFR. Used in the lambda model
/// (lambda = MAF / (IQ * 14.5)).
pub const DIESEL_AFR_STOICH: f64 = 14.5;

/// Modelled-torque conversion (re-exported from `stock_refs` for ergonomics).
pub const NM_PER_MG_IQ: f64 = super::stock_refs::NM_PER_MG_IQ;

/// Numeric envelope caps. Values are absolute caps the tool must NEVER
/// exceed.
#[derive(Debug, Clone, Copy)]
pub struct EnvelopeCaps {
    /// Right edge of efficient compressor map, mbar absolute.
    pub peak_boost_mbar_abs: i32,
    /// Tighter cap above 4000 rpm; KP35 chokes there.
    pub peak_boost_above_4000rpm_mbar_abs: i32,

    /// Stock-injector duration headroom + LUK clutch torque ceiling.
    pub peak_iq_mg: f64,
    /// Lambda floor; below this PD smokes.
    pub lambda_floor: f64,
    /// Cast-iron manifold creep + AMF has no oil-jet pistons.
    pub pre_turbo_egt_c_sustained: i32,

    /// SOI advance cap (deg BTDC) at IQ >= `soi_iq_threshold_mg`.
    pub soi_max_btdc: f64,
    /// IQ at and above which the SOI cap kicks in.
    pub soi_iq_threshold_mg: f64,
    /// EOI cap (deg ATDC); past this, heat dumps into the turbine.
    pub eoi_max_atdc: f64,

    /// LUK SMF clutch ceiling (Nm flywheel).
    pub modelled_flywheel_torque_nm: f64,

    /// HFM5 non-linear above this MAF reading.
    pub maf_max_mg_stroke: f64,

    /// SVBL change cap — never touch the overboost cut.
    pub svbl_change_mbar: i32,
}

/// Canonical, frozen instance of [`EnvelopeCaps`].
pub const CAPS: EnvelopeCaps = EnvelopeCaps {
    peak_boost_mbar_abs: 2150,
    peak_boost_above_4000rpm_mbar_abs: 2050,
    peak_iq_mg: 52.0,
    lambda_floor: 1.20,
    pre_turbo_egt_c_sustained: 800,
    soi_max_btdc: 26.0,
    soi_iq_threshold_mg: 30.0,
    eoi_max_atdc: 10.0,
    modelled_flywheel_torque_nm: 240.0,
    maf_max_mg_stroke: 1000.0,
    svbl_change_mbar: 0,
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
                "KP35 chokes above 4000 rpm; sustained PR > 2.0 over-speeds the shaft. \
                 Capped at {} mbar abs.",
                CAPS.peak_boost_above_4000rpm_mbar_abs
            ),
        );
    }
    if proposed_mbar_abs > f64::from(CAPS.peak_boost_mbar_abs) {
        return ClampOutcome::blocked(
            f64::from(CAPS.peak_boost_mbar_abs),
            "peak_boost_mbar_abs",
            format!(
                "Right edge of KP35 efficient compressor map at AMF flow rates. \
                 Capped at {} mbar abs.",
                CAPS.peak_boost_mbar_abs
            ),
        );
    }
    ClampOutcome::ok(proposed_mbar_abs)
}

/// Cap requested IQ by the stock-injector + LUK clutch ceiling.
pub fn clamp_iq(proposed_mg: f64) -> ClampOutcome {
    if proposed_mg > CAPS.peak_iq_mg {
        return ClampOutcome::blocked(
            CAPS.peak_iq_mg,
            "peak_iq_mg",
            format!(
                "Stock injector duration headroom + LUK clutch torque ceiling. \
                 Capped at {} mg/stroke.",
                CAPS.peak_iq_mg
            ),
        );
    }
    ClampOutcome::ok(proposed_mg)
}

/// Cap SOI advance.
///
/// Below the IQ threshold there is more thermal margin and slightly more
/// advance is survivable, but for safety the cap is applied at every IQ.
pub fn clamp_soi(proposed_btdc: f64, _iq_mg: f64) -> ClampOutcome {
    if proposed_btdc > CAPS.soi_max_btdc {
        return ClampOutcome::blocked(
            CAPS.soi_max_btdc,
            "soi_max_btdc",
            format!(
                "At IQ >= {} mg, advance beyond {}° BTDC migrates peak cylinder \
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
                "LUK SMF clutch ceiling (195 Nm rated × 1.23 headroom). \
                 Capped at {} Nm.",
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
                "Below λ = {} on PD = visible smoke + EGT spike + DPF/cat damage. \
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
        let r = clamp_iq(52.0);
        assert!(!r.blocked);
    }

    #[test]
    fn iq_above_cap_blocked() {
        let r = clamp_iq(55.0);
        assert!(r.blocked);
        assert_eq!(r.value, 52.0);
    }

    #[test]
    fn soi_above_cap_blocked() {
        let r = clamp_soi(28.0, 45.0);
        assert!(r.blocked);
        assert_eq!(r.value, 26.0);
    }

    #[test]
    fn torque_above_cap_blocked() {
        let r = clamp_torque_nm(260.0);
        assert!(r.blocked);
        assert_eq!(r.value, 240.0);
    }

    #[test]
    fn lambda_below_floor_blocked() {
        let r = clamp_lambda_floor(1.10);
        assert!(r.blocked);
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
}
