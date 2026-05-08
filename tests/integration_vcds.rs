//! End-to-end ingest tests using the on-disk VCDS fixtures.

use std::path::PathBuf;

use ecu_shenanigans::ingest::parse_vcds_csv;
use ecu_shenanigans::util::{detect_pulls, resample_to_uniform};
use ecu_shenanigans::util::timebase::DEFAULT_RATE_HZ;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests").join("fixtures").join(name)
}

#[test]
fn healthy_pull_fixture_parses_cleanly() {
    let p = fixture("vcds_amf_001_003_011.csv");
    let log = parse_vcds_csv(&p).expect("parse healthy fixture");
    assert!(log.groups.contains("001"));
    assert!(log.groups.contains("003"));
    assert!(log.groups.contains("011"));
    assert!(!log.is_empty(), "fixture had data");
    // Group 008 is intentionally absent in this fixture; check the
    // missing-groups diagnostic surfaces it.
    let missing = log.missing_required_groups();
    assert_eq!(missing, vec!["008".to_string()]);
    assert!(log.data.contains_key("rpm"));
    assert!(log.data.contains_key("boost_actual"));
}

#[test]
fn overboost_fixture_parses_and_pulls_detected() {
    let p = fixture("vcds_amf_008_011.csv");
    let log = parse_vcds_csv(&p).expect("parse overboost fixture");
    assert!(log.has_required_groups() || log.groups.contains("011"));
    let df = resample_to_uniform(&log, DEFAULT_RATE_HZ);
    let pulls = detect_pulls(&df);
    assert!(!pulls.is_empty(), "overboost fixture must contain at least one pull");
}

#[test]
fn lambda_fixture_parses() {
    let p = fixture("vcds_amf_020_021.csv");
    let log = parse_vcds_csv(&p).expect("parse lambda fixture");
    assert!(!log.is_empty());
    let df = resample_to_uniform(&log, DEFAULT_RATE_HZ);
    assert!(df.len() >= log.len(),
        "resampled grid is at least as dense as the source");
}

#[test]
fn unknown_path_returns_io_error() {
    let p = fixture("does_not_exist.csv");
    let err = parse_vcds_csv(&p).expect_err("missing file should error");
    let msg = err.to_string();
    assert!(msg.contains("could not read"), "error mentions path: {msg}");
}
