//! Pull-detection sanity checks against the fixtures.

use std::path::PathBuf;

use ecu_shenanigans::ingest::parse_vcds_csv;
use ecu_shenanigans::util::{detect_pulls, resample_to_uniform};
use ecu_shenanigans::util::timebase::DEFAULT_RATE_HZ;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests").join("fixtures").join(name)
}

#[test]
fn detected_pulls_have_monotonic_ids() {
    let log = parse_vcds_csv(fixture("vcds_amf_008_011.csv")).unwrap();
    let df = resample_to_uniform(&log, DEFAULT_RATE_HZ);
    let pulls = detect_pulls(&df);
    if pulls.is_empty() { return; }
    for (i, p) in pulls.iter().enumerate() {
        assert_eq!(p.pull_id as usize, i + 1);
    }
}

#[test]
fn detected_pulls_satisfy_minimum_duration() {
    let log = parse_vcds_csv(fixture("vcds_amf_020_021.csv")).unwrap();
    let df = resample_to_uniform(&log, DEFAULT_RATE_HZ);
    let pulls = detect_pulls(&df);
    for p in &pulls {
        assert!(p.duration_s() >= 2.0,
            "pull {} too short: {}s", p.pull_id, p.duration_s());
    }
}

#[test]
fn resample_preserves_endpoints() {
    let log = parse_vcds_csv(fixture("vcds_amf_001_003_011.csv")).unwrap();
    let raw_t0 = log.time[0];
    let raw_tn = log.time[log.time.len() - 1];
    let df = resample_to_uniform(&log, DEFAULT_RATE_HZ);
    let new_t0 = df.time[0];
    let new_tn = df.time[df.time.len() - 1];
    assert!((new_t0 - raw_t0).abs() < 1e-9);
    assert!((new_tn - raw_tn).abs() < 1.0 / DEFAULT_RATE_HZ + 1e-9);
}
