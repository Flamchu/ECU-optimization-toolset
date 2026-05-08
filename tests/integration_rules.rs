//! Per-rule unit tests using hand-crafted micro-fixtures.

use std::collections::BTreeMap;

use ecu_shenanigans::rules::base::Severity;
use ecu_shenanigans::rules::pack::{
    r02_overboost_spike, r03_boost_target_excessive, r04_no_taper,
    r05_maf_below_spec, r06_lambda_floor, r07_peak_iq, r08_torque_ceiling,
    r09_soi_excess, r10_eoi_late, r11_coolant_low, r12_no_atm,
    r13_fuel_temp, r14_srcv, r15_limp,
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
fn r07_fires_when_iq_above_envelope() {
    let iq = vec![55.0; N];
    let log = log_from(&[("iq_requested", iq)]);
    let f = r07_peak_iq(&log, &full_pull());
    assert_eq!(f.len(), 1);
    assert!(f[0].observed_extreme > 52.0);
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
