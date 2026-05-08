//! Per-rule unit tests using hand-crafted micro-fixtures.

use std::collections::BTreeMap;

use ecu_shenanigans::rules::base::Severity;
use ecu_shenanigans::rules::pack::{
    r02_overboost_spike, r03_boost_target_excessive, r04_no_taper,
    r05_maf_below_spec, r06_lambda_floor, r07_peak_iq, r08_torque_ceiling,
    r09_soi_excess, r10_eoi_late, r11_coolant_low, r12_no_atm,
    r13_fuel_temp, r14_srcv, r15_limp,
    r16_egr_observed, r17_maf_deviation, r17b_maf_excess_info,
    r18_cruise_nvh, r19_dtc_scan,
};
use ecu_shenanigans::util::{Pull, ResampledLog};

const N: usize = 30;
const DT: f64 = 0.2;

fn time() -> Vec<f64> {
    (0..N).map(|i| (i as f64) * DT).collect()
}

fn full_pull() -> Pull {
    Pull {
        pull_id: 1, i_start: 0, i_end: N,
        t_start: 0.0, t_end: ((N - 1) as f64) * DT,
        rpm_start: 2000.0, rpm_end: 4500.0,
    }
}

fn log_from(channels: &[(&str, Vec<f64>)]) -> ResampledLog {
    let mut data: BTreeMap<String, Vec<f64>> = BTreeMap::new();
    for (k, v) in channels {
        data.insert((*k).to_string(), v.clone());
    }
    ResampledLog { time: time(), data }
}

fn rpm_ramp() -> Vec<f64> {
    (0..N).map(|i| 2000.0 + (i as f64) * (2500.0 / (N - 1) as f64)).collect()
}

#[test]
fn r02_fires_on_overboost_spike() {
    let mut actual = vec![1500.0; N];
    let spec = vec![1500.0; N];
    actual[15] = 2400.0;
    let log = log_from(&[("boost_actual", actual), ("boost_spec", spec)]);
    let f = r02_overboost_spike(&log, &full_pull());
    assert_eq!(f.len(), 1);
    assert_eq!(f[0].severity, Severity::Critical);
}

#[test]
fn r02_quiet_when_within_envelope() {
    let actual = vec![1500.0; N];
    let spec = vec![1500.0; N];
    let log = log_from(&[("boost_actual", actual), ("boost_spec", spec)]);
    let f = r02_overboost_spike(&log, &full_pull());
    assert!(f.is_empty());
}

#[test]
fn r03_fires_on_excessive_target() {
    let spec = vec![2200.0; N]; // > 2150 cap
    let log = log_from(&[("rpm", rpm_ramp()), ("boost_spec", spec)]);
    let f = r03_boost_target_excessive(&log, &full_pull());
    assert_eq!(f.len(), 1);
    assert_eq!(f[0].severity, Severity::Critical);
}

#[test]
fn r04_fires_when_high_rpm_does_not_taper() {
    let mut rpm = vec![3000.0; N];
    let mut spec = vec![2000.0; N];
    for (i, r) in rpm.iter_mut().enumerate() {
        if i >= 15 { *r = 4500.0; }
    }
    for (i, s) in spec.iter_mut().enumerate() {
        if i >= 15 { *s = 1980.0; }
    }
    let log = log_from(&[("rpm", rpm), ("boost_spec", spec)]);
    let f = r04_no_taper(&log, &full_pull());
    assert!(!f.is_empty());
}

#[test]
fn r05_fires_on_maf_deficit() {
    let spec = vec![800.0; N];
    let actual = vec![700.0; N]; // ~12.5 % deficit > 8 %
    let log = log_from(&[("maf_actual", actual), ("maf_spec", spec)]);
    let f = r05_maf_below_spec(&log, &full_pull());
    assert_eq!(f.len(), 1);
}

#[test]
fn r06_fires_when_lambda_below_floor() {
    let maf = vec![500.0; N];
    let iq = vec![45.0; N]; // λ ≈ 500 / (45 * 14.5) ≈ 0.766 — way below 1.20
    let log = log_from(&[("maf_actual", maf), ("iq_requested", iq)]);
    let f = r06_lambda_floor(&log, &full_pull());
    assert_eq!(f.len(), 1);
    assert_eq!(f[0].severity, Severity::Critical);
}

#[test]
fn r07_fires_when_iq_above_v3_envelope() {
    let iq = vec![56.0; N];
    let log = log_from(&[("iq_requested", iq)]);
    let f = r07_peak_iq(&log, &full_pull());
    assert_eq!(f.len(), 1);
    assert!(f[0].observed_extreme > 54.0);
}

#[test]
fn r07_quiet_at_v2_old_threshold() {
    // 53 mg/stroke would have tripped v2 (cap=52) but is fine in v3 (cap=54).
    let iq = vec![53.0; N];
    let log = log_from(&[("iq_requested", iq)]);
    let f = r07_peak_iq(&log, &full_pull());
    assert!(f.is_empty());
}

#[test]
fn r06_quiet_at_v2_lambda_floor() {
    // λ ≈ 1.10 was a v2 critical; v3 floor is 1.05 so it should pass.
    let maf = vec![1100.0; N];
    let iq = vec![69.0; N]; // λ ≈ 1100 / (69 * 14.5) ≈ 1.099
    let log = log_from(&[("maf_actual", maf), ("iq_requested", iq)]);
    let f = r06_lambda_floor(&log, &full_pull());
    assert!(f.is_empty(), "λ ≈ 1.10 should be allowed in v3");
}

#[test]
fn r08_fires_when_modelled_torque_above_clutch() {
    let iq = vec![60.0; N]; // 60 * 4.4 = 264 Nm > 240
    let log = log_from(&[("iq_requested", iq)]);
    let f = r08_torque_ceiling(&log, &full_pull());
    assert_eq!(f.len(), 1);
}

#[test]
fn r09_fires_at_high_soi_and_iq() {
    let soi = vec![28.0; N];
    let iq = vec![45.0; N];
    let log = log_from(&[("soi_actual", soi), ("iq_requested", iq)]);
    let f = r09_soi_excess(&log, &full_pull(), false);
    assert_eq!(f.len(), 1);
    assert_eq!(f[0].severity, Severity::Critical);
}

#[test]
fn r09_downgrades_to_warn_on_low_rate() {
    let soi = vec![28.0; N];
    let iq = vec![45.0; N];
    let log = log_from(&[("soi_actual", soi), ("iq_requested", iq)]);
    let f = r09_soi_excess(&log, &full_pull(), true);
    assert_eq!(f.len(), 1);
    assert_eq!(f[0].severity, Severity::Warn);
}

#[test]
fn r10_skipped_without_required_channels() {
    let log = log_from(&[("rpm", rpm_ramp())]);
    let f = r10_eoi_late(&log, &full_pull(), false);
    assert!(f.iter().any(|x| x.skipped));
}

#[test]
fn r11_fires_when_coolant_below_80() {
    let c = vec![60.0; N];
    let log = log_from(&[("coolant_c", c)]);
    let f = r11_coolant_low(&log, &full_pull());
    assert_eq!(f.len(), 1);
    assert_eq!(f[0].severity, Severity::Info);
}

#[test]
fn r12_fires_when_atm_pressure_missing() {
    let log = log_from(&[("rpm", rpm_ramp())]);
    let f = r12_no_atm(&log, &full_pull());
    assert_eq!(f.len(), 1);
    assert_eq!(f[0].severity, Severity::Info);
}

#[test]
fn r12_quiet_when_atm_present() {
    let log = log_from(&[("atm_pressure", vec![1013.0; N])]);
    let f = r12_no_atm(&log, &full_pull());
    assert!(f.is_empty());
}

#[test]
fn r13_fires_when_fuel_temp_above_80() {
    let log = log_from(&[("fuel_temp_c", vec![85.0; N])]);
    let f = r13_fuel_temp(&log, &full_pull());
    assert_eq!(f.len(), 1);
}

#[test]
fn r14_fires_when_cylinder_deviates() {
    let mut c1 = vec![0.0; N];
    let c2 = vec![0.0; N];
    let c3 = vec![0.0; N];
    c1[10] = 4.0; // huge deviation from mean
    let log = log_from(&[
        ("srcv_cyl1", c1), ("srcv_cyl2", c2), ("srcv_cyl3", c3),
    ]);
    let f = r14_srcv(&log, &full_pull());
    assert_eq!(f.len(), 1);
}

#[test]
fn r15_fires_when_n75_clamped() {
    let log = log_from(&[("n75_duty", vec![50.0; N])]);
    let f = r15_limp(&log, &full_pull());
    assert_eq!(f.len(), 1);
}

#[test]
fn r15_quiet_when_n75_modulates() {
    let mut n75 = vec![50.0; N];
    for (i, x) in n75.iter_mut().enumerate() {
        *x += (i as f64).sin() * 5.0;
    }
    let log = log_from(&[("n75_duty", n75)]);
    let f = r15_limp(&log, &full_pull());
    assert!(f.is_empty());
}

// ---------------------------------------------------------------------------
// v3 EGR-delete rule tests
// ---------------------------------------------------------------------------

#[test]
fn r16_fires_when_egr_observed_post_delete() {
    let mut duty = vec![0.0; N];
    duty[10] = 35.0; // delete clearly not applied
    let log = log_from(&[("egr_duty", duty)]);
    let f = r16_egr_observed(&log, &full_pull());
    assert_eq!(f.len(), 1);
    assert_eq!(f[0].severity, Severity::Critical);
    assert!(f[0].observed_extreme > 30.0);
}

#[test]
fn r16_quiet_when_egr_zero() {
    let log = log_from(&[("egr_duty", vec![0.5; N])]); // sensor noise
    let f = r16_egr_observed(&log, &full_pull());
    assert!(f.is_empty());
}

#[test]
fn r16_skipped_when_channel_missing() {
    let log = log_from(&[("rpm", rpm_ramp())]);
    let f = r16_egr_observed(&log, &full_pull());
    assert!(f.iter().any(|x| x.skipped));
}

#[test]
fn r17_fires_on_sustained_maf_deviation() {
    let actual = vec![100.0; N]; // 80% deviation, sustained
    let spec = vec![500.0; N];
    let coolant = vec![85.0; N];
    let pedal = vec![25.0; N]; // cruise, not WOT
    let log = log_from(&[
        ("maf_actual", actual), ("maf_spec", spec),
        ("coolant_c", coolant), ("tps_pct", pedal),
    ]);
    let f = r17_maf_deviation(&log, &full_pull());
    assert_eq!(f.len(), 1);
    assert_eq!(f[0].severity, Severity::Warn);
}

#[test]
fn r17_excludes_wot_samples() {
    let actual = vec![100.0; N];
    let spec = vec![500.0; N];
    let coolant = vec![85.0; N];
    let pedal = vec![95.0; N]; // WOT — exclude
    let log = log_from(&[
        ("maf_actual", actual), ("maf_spec", spec),
        ("coolant_c", coolant), ("tps_pct", pedal),
    ]);
    let f = r17_maf_deviation(&log, &full_pull());
    assert!(f.is_empty(), "WOT samples must be excluded from R17");
}

#[test]
fn r17_excludes_cold_start_samples() {
    let actual = vec![100.0; N];
    let spec = vec![500.0; N];
    let coolant = vec![40.0; N]; // cold — exclude
    let pedal = vec![25.0; N];
    let log = log_from(&[
        ("maf_actual", actual), ("maf_spec", spec),
        ("coolant_c", coolant), ("tps_pct", pedal),
    ]);
    let f = r17_maf_deviation(&log, &full_pull());
    assert!(f.is_empty(), "cold-start samples must be excluded from R17");
}

#[test]
fn r17b_fires_when_maf_exceeds_spec_with_egr_zero() {
    // Strategy-B post-delete: spec saturated at 850, MAF actual at 350-650.
    // We need actual > spec + 50, so spec must be <300 for any chance.
    // Realistic Strategy-A re-scaled spec: spec=200, actual=300 → excess 100.
    let actual = vec![300.0; N];
    let spec = vec![200.0; N];
    let egr = vec![0.0; N];
    let log = log_from(&[
        ("maf_actual", actual), ("maf_spec", spec), ("egr_duty", egr),
    ]);
    let f = r17b_maf_excess_info(&log, &full_pull());
    assert_eq!(f.len(), 1);
    assert_eq!(f[0].severity, Severity::Info);
}

#[test]
fn r17b_silent_when_egr_active() {
    let actual = vec![300.0; N];
    let spec = vec![200.0; N];
    let egr = vec![25.0; N]; // EGR active — don't claim "delete functional"
    let log = log_from(&[
        ("maf_actual", actual), ("maf_spec", spec), ("egr_duty", egr),
    ]);
    let f = r17b_maf_excess_info(&log, &full_pull());
    assert!(f.is_empty());
}

#[test]
fn r18_fires_at_warm_cruise_with_stock_soi_and_egr_zero() {
    let n = N;
    let rpm = vec![2000.0; n];
    let iq = vec![10.0; n];
    let soi = vec![20.0; n]; // > 18° BTDC stock cruise
    let egr = vec![0.0; n];
    let coolant = vec![85.0; n];
    let pedal = vec![20.0; n];
    let log = log_from(&[
        ("rpm", rpm), ("iq_requested", iq), ("soi_actual", soi),
        ("egr_duty", egr), ("coolant_c", coolant), ("tps_pct", pedal),
    ]);
    let f = r18_cruise_nvh(&log, &full_pull());
    assert_eq!(f.len(), 1);
    assert_eq!(f[0].severity, Severity::Info);
}

#[test]
fn r18_quiet_outside_cruise_band() {
    let n = N;
    let rpm = vec![3500.0; n]; // outside cruise band
    let iq = vec![10.0; n];
    let soi = vec![20.0; n];
    let egr = vec![0.0; n];
    let log = log_from(&[
        ("rpm", rpm), ("iq_requested", iq), ("soi_actual", soi), ("egr_duty", egr),
    ]);
    let f = r18_cruise_nvh(&log, &full_pull());
    assert!(f.is_empty());
}

#[test]
fn r19_fires_on_egr_dtcs() {
    // Encode P0401 as 401, P0403 as 403 in the dtc_codes channel.
    let mut dtc = vec![f64::NAN; N];
    dtc[5] = 401.0; // P0401
    dtc[10] = 403.0; // P0403 wiring fault
    let log = log_from(&[("dtc_codes", dtc)]);
    let f = r19_dtc_scan(&log, &full_pull());
    assert_eq!(f.len(), 1);
    assert_eq!(f[0].severity, Severity::Warn);
    assert!(f[0].recommended_action_ref.as_ref().unwrap().contains("P0403"));
}

#[test]
fn r19_quiet_when_no_egr_dtcs_present() {
    let mut dtc = vec![f64::NAN; N];
    dtc[5] = 300.0; // some other code, not in our list
    let log = log_from(&[("dtc_codes", dtc)]);
    let f = r19_dtc_scan(&log, &full_pull());
    assert!(f.is_empty());
}
