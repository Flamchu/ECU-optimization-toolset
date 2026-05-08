//! End-to-end driveability + thermal tests for R22 (low-pedal IQ slope)
//! and R23 (sustained-pull coolant trend), using synthetic logs.

use std::collections::BTreeMap;

use ecu_shenanigans::rules::base::Severity;
use ecu_shenanigans::rules::pack::{r22_low_pedal_slope, r23_coolant_trend};
use ecu_shenanigans::util::{Pull, ResampledLog};

const DT: f64 = 0.2;

fn synth_log(channels: &[(&str, Vec<f64>)]) -> ResampledLog {
    let mut data: BTreeMap<String, Vec<f64>> = BTreeMap::new();
    let n = channels.first().map_or(0, |(_, v)| v.len());
    for (k, v) in channels {
        data.insert((*k).to_string(), v.clone());
    }
    let time: Vec<f64> = (0..n).map(|i| i as f64 * DT).collect();
    ResampledLog { time, data }
}

fn pull_spanning(n: usize) -> Pull {
    Pull {
        pull_id: 1, i_start: 0, i_end: n,
        t_start: 0.0, t_end: ((n - 1) as f64) * DT,
        rpm_start: 1500.0, rpm_end: 4000.0,
    }
}

// ---------------------------------------------------------------------------
// R22 — low-pedal IQ slope
// ---------------------------------------------------------------------------

#[test]
fn r22_fires_warn_on_steep_low_pedal_slope() {
    // Low-pedal lunge: 30 samples in 6..24 % pedal where iq jumps from 5 to
    // 20 mg/stroke → slope ≈ 0.83 mg per pct, well above the 0.40 cap.
    let n = 90;
    let mut pedal: Vec<f64> = Vec::with_capacity(n);
    let mut iq: Vec<f64> = Vec::with_capacity(n);
    for i in 0..n {
        if i < 30 {
            // Low-band sweep: pedal 6..24 %, iq 5..20 mg.
            let p = 6.0 + ((i as f64) / 30.0) * 18.0;
            let q = 5.0 + ((i as f64) / 30.0) * 15.0;
            pedal.push(p);
            iq.push(q);
        } else if i < 60 {
            // Mid-band sweep: pedal 26..78 %, iq 22..32 mg (slope ≈ 0.19).
            let p = 26.0 + ((i - 30) as f64 / 30.0) * 52.0;
            let q = 22.0 + ((i - 30) as f64 / 30.0) * 10.0;
            pedal.push(p);
            iq.push(q);
        } else {
            // WOT: pedal 90 %, iq 45 mg.
            pedal.push(90.0);
            iq.push(45.0);
        }
    }
    let rpm = vec![2000.0; n];
    let coolant = vec![85.0; n];
    let log = synth_log(&[
        ("pedal_pct", pedal),
        ("iq_requested", iq),
        ("rpm", rpm),
        ("coolant_c", coolant),
    ]);
    let f = r22_low_pedal_slope(&log, &pull_spanning(n));
    assert_eq!(f.len(), 1);
    assert!(!f[0].skipped);
    assert_eq!(f[0].severity, Severity::Warn);
}

#[test]
fn r22_quiet_on_gentle_low_pedal_slope() {
    // Gentle: low-band slope ≈ 0.20 mg per pct, below the 0.40 cap; mid-band
    // similar so the ratio test does not fire either.
    let n = 90;
    let mut pedal: Vec<f64> = Vec::with_capacity(n);
    let mut iq: Vec<f64> = Vec::with_capacity(n);
    for i in 0..n {
        if i < 30 {
            let p = 6.0 + ((i as f64) / 30.0) * 18.0;
            let q = 4.0 + ((i as f64) / 30.0) * 4.0; // 4..8 mg over 18 pct
            pedal.push(p);
            iq.push(q);
        } else if i < 60 {
            let p = 26.0 + ((i - 30) as f64 / 30.0) * 52.0;
            let q = 10.0 + ((i - 30) as f64 / 30.0) * 12.0;
            pedal.push(p);
            iq.push(q);
        } else {
            pedal.push(90.0);
            iq.push(45.0);
        }
    }
    let log = synth_log(&[
        ("pedal_pct", pedal),
        ("iq_requested", iq),
        ("rpm", vec![2000.0; n]),
        ("coolant_c", vec![85.0; n]),
    ]);
    let f = r22_low_pedal_slope(&log, &pull_spanning(n));
    assert!(f.is_empty(), "gentle slope must not fire R22");
}

#[test]
fn r22_skipped_when_pedal_pct_missing() {
    let n = 60;
    let log = synth_log(&[
        ("iq_requested", vec![10.0; n]),
        ("rpm", vec![2000.0; n]),
    ]);
    let f = r22_low_pedal_slope(&log, &pull_spanning(n));
    assert_eq!(f.len(), 1);
    assert!(f[0].skipped);
    assert!(f[0].rationale.to_ascii_lowercase().contains("pedal_pct"));
}

#[test]
fn r22_skipped_when_low_band_too_sparse() {
    // Only 5 low-band samples — under the 30-sample minimum.
    let n = 60;
    let mut pedal = vec![80.0; n];
    for (i, slot) in pedal.iter_mut().enumerate().take(5) {
        *slot = 10.0 + (i as f64);
    }
    let log = synth_log(&[
        ("pedal_pct", pedal),
        ("iq_requested", vec![15.0; n]),
        ("rpm", vec![2000.0; n]),
        ("coolant_c", vec![85.0; n]),
    ]);
    let f = r22_low_pedal_slope(&log, &pull_spanning(n));
    assert_eq!(f.len(), 1);
    assert!(f[0].skipped);
}

#[test]
fn r22_skipped_when_cold() {
    // All samples cold (coolant < 70 °C) → no qualifying samples → SKIP.
    let n = 60;
    let pedal: Vec<f64> = (0..n).map(|i| 6.0 + (i as f64 * 0.3)).collect();
    let log = synth_log(&[
        ("pedal_pct", pedal),
        ("iq_requested", vec![15.0; n]),
        ("rpm", vec![2000.0; n]),
        ("coolant_c", vec![40.0; n]),
    ]);
    let f = r22_low_pedal_slope(&log, &pull_spanning(n));
    assert_eq!(f.len(), 1);
    assert!(f[0].skipped);
}

// ---------------------------------------------------------------------------
// R23 — sustained-pull coolant trend
// ---------------------------------------------------------------------------

#[test]
fn r23_fires_warn_when_coolant_climbs_into_warn_band() {
    // Coolant climbs from 90 to 104 °C across the pull at high rpm and
    // pedal — should fire Warn (peak ≥ 102, dT ≥ 10).
    let n = 60;
    let pedal = vec![80.0; n];
    let rpm = vec![3500.0; n];
    let coolant: Vec<f64> = (0..n).map(|i| 90.0 + (i as f64 / (n - 1) as f64) * 14.0).collect();
    let log = synth_log(&[
        ("pedal_pct", pedal),
        ("rpm", rpm),
        ("coolant_c", coolant),
    ]);
    let f = r23_coolant_trend(&log, &pull_spanning(n));
    assert_eq!(f.len(), 1);
    assert_eq!(f[0].severity, Severity::Warn);
}

#[test]
fn r23_emits_info_when_coolant_climbs_but_stays_below_warn() {
    // dT = 12 (≥ 10 arming), peak = 100 (< 102 warn) → Info.
    let n = 60;
    let coolant: Vec<f64> = (0..n).map(|i| 88.0 + (i as f64 / (n - 1) as f64) * 12.0).collect();
    let log = synth_log(&[
        ("pedal_pct", vec![80.0; n]),
        ("rpm", vec![3500.0; n]),
        ("coolant_c", coolant),
    ]);
    let f = r23_coolant_trend(&log, &pull_spanning(n));
    assert_eq!(f.len(), 1);
    assert_eq!(f[0].severity, Severity::Info);
}

#[test]
fn r23_quiet_when_dt_below_arm() {
    // dT < 10 °C → rule armed but not triggered.
    let n = 60;
    let coolant: Vec<f64> = (0..n).map(|i| 92.0 + (i as f64 / (n - 1) as f64) * 5.0).collect();
    let log = synth_log(&[
        ("pedal_pct", vec![80.0; n]),
        ("rpm", vec![3500.0; n]),
        ("coolant_c", coolant),
    ]);
    let f = r23_coolant_trend(&log, &pull_spanning(n));
    assert!(f.is_empty(), "small dT should not fire R23 at any severity");
}

#[test]
fn r23_skipped_when_coolant_missing() {
    let n = 60;
    let log = synth_log(&[
        ("pedal_pct", vec![80.0; n]),
        ("rpm", vec![3500.0; n]),
    ]);
    let f = r23_coolant_trend(&log, &pull_spanning(n));
    assert_eq!(f.len(), 1);
    assert!(f[0].skipped);
}

#[test]
fn r23_skipped_when_rpm_never_high_enough() {
    // rpm < 2500 throughout — not a real pull.
    let n = 60;
    let log = synth_log(&[
        ("pedal_pct", vec![80.0; n]),
        ("rpm", vec![1500.0; n]),
        ("coolant_c", (0..n).map(|i| 90.0 + i as f64 * 0.3).collect()),
    ]);
    let f = r23_coolant_trend(&log, &pull_spanning(n));
    assert_eq!(f.len(), 1);
    assert!(f[0].skipped);
}

#[test]
fn r23_skipped_when_pull_too_short() {
    // Pull shorter than 5 s.
    let n = 10; // 10 × 0.2 = 2 s
    let log = synth_log(&[
        ("pedal_pct", vec![80.0; n]),
        ("rpm", vec![3500.0; n]),
        ("coolant_c", vec![100.0; n]),
    ]);
    let f = r23_coolant_trend(&log, &pull_spanning(n));
    assert_eq!(f.len(), 1);
    assert!(f[0].skipped);
}
