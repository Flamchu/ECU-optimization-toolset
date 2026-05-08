//! Property tests: every clamp_* function must keep values inside the
//! envelope no matter what input it sees.

use ecu_shenanigans::platform::amf_edc15p::envelope::{
    clamp_boost_target, clamp_eoi_atdc, clamp_iq, clamp_lambda_floor,
    clamp_soi, clamp_svbl, clamp_torque_nm, CAPS,
};
use proptest::prelude::*;

fn finite_f64() -> impl Strategy<Value = f64> {
    prop::num::f64::ANY.prop_filter("finite only", |x| x.is_finite())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(1024))]

    #[test]
    fn boost_clamp_stays_inside_envelope(
        proposed in finite_f64(),
        rpm in 0.0_f64..7000.0,
    ) {
        let r = clamp_boost_target(proposed, rpm);
        if rpm >= 4000.0 {
            prop_assert!(r.value <= f64::from(CAPS.peak_boost_above_4000rpm_mbar_abs) + 1e-9);
        } else {
            prop_assert!(r.value <= f64::from(CAPS.peak_boost_mbar_abs) + 1e-9);
        }
    }

    #[test]
    fn iq_clamp_never_exceeds_cap(proposed in finite_f64()) {
        let r = clamp_iq(proposed);
        prop_assert!(r.value <= CAPS.peak_iq_mg + 1e-9);
    }

    #[test]
    fn soi_clamp_never_exceeds_cap(
        proposed in finite_f64(),
        iq in 0.0_f64..80.0,
    ) {
        let r = clamp_soi(proposed, iq);
        prop_assert!(r.value <= CAPS.soi_max_btdc + 1e-9);
    }

    #[test]
    fn torque_clamp_never_exceeds_cap(proposed in finite_f64()) {
        let r = clamp_torque_nm(proposed);
        prop_assert!(r.value <= CAPS.modelled_flywheel_torque_nm + 1e-9);
    }

    #[test]
    fn lambda_clamp_never_below_floor(proposed in finite_f64()) {
        let r = clamp_lambda_floor(proposed);
        prop_assert!(r.value >= CAPS.lambda_floor - 1e-9);
    }

    #[test]
    fn svbl_clamp_only_passes_zero(proposed in finite_f64()) {
        let r = clamp_svbl(proposed);
        prop_assert!(r.value == 0.0);
    }

    #[test]
    fn eoi_clamp_never_exceeds_cap(proposed in finite_f64()) {
        let r = clamp_eoi_atdc(proposed);
        prop_assert!(r.value <= CAPS.eoi_max_atdc + 1e-9);
    }
}

#[test]
fn boost_clamp_at_exact_cap_passes() {
    let r = clamp_boost_target(2150.0, 3000.0);
    assert!(!r.blocked);
}

#[test]
fn boost_clamp_one_above_cap_blocks() {
    let r = clamp_boost_target(2150.5, 3000.0);
    assert!(r.blocked);
}

// Compile-time invariants that the envelope caps must satisfy. If any of
// these become false the build fails — no need for a runtime assertion.
const _SELF_CONSISTENT: () = {
    assert!(CAPS.peak_boost_above_4000rpm_mbar_abs <= CAPS.peak_boost_mbar_abs,
        "above-4000 cap must be tighter or equal to the global cap");
    assert!(CAPS.eoi_max_atdc > 0.0, "EOI cap must be positive");
};
