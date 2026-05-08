//! v4: DTC sidecar ingest + R19 firing logic.

use std::path::PathBuf;
use std::io::Write;

use ecu_shenanigans::ingest::{
    parse_dtc_text, parse_vcds_csv, parse_vcds_csv_with_dtc, read_sidecar, sidecar_path_for,
};
use ecu_shenanigans::rules::analyse;
use ecu_shenanigans::util::resample_to_uniform;
use ecu_shenanigans::util::timebase::DEFAULT_RATE_HZ;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests").join("fixtures").join(name)
}

#[test]
fn parse_dtc_text_basic() {
    let codes = parse_dtc_text("P0401\nP0403\n");
    assert_eq!(codes, vec!["P0401", "P0403"]);
}

#[test]
fn parse_dtc_text_with_descriptions() {
    let txt = "# DTC scan from VCDS\nP0401  EGR insufficient flow\nP0403\tsolenoid\n";
    let codes = parse_dtc_text(txt);
    assert_eq!(codes, vec!["P0401", "P0403"]);
}

#[test]
fn sidecar_path_convention() {
    let p = sidecar_path_for("/tmp/vcds_amf_pre_delete.csv");
    assert_eq!(p.file_name().unwrap(), "vcds_amf_pre_delete.dtc.txt");
}

#[test]
fn missing_sidecar_returns_empty_vec() {
    let p = std::env::temp_dir().join("no_such_dtc_file_xyz_v4.dtc.txt");
    let _ = std::fs::remove_file(&p);
    let codes = read_sidecar(&p).unwrap();
    assert!(codes.is_empty());
}

#[test]
fn parse_vcds_with_explicit_dtc_path() {
    let dir = std::env::temp_dir();
    let dtc_path = dir.join("ecu_shenanigans_test_v4_explicit.dtc.txt");
    {
        let mut f = std::fs::File::create(&dtc_path).unwrap();
        f.write_all(b"P0401\nP0403\n").unwrap();
    }
    let log = parse_vcds_csv_with_dtc(fixture("vcds_amf_post_delete.csv"), &dtc_path).unwrap();
    assert_eq!(log.dtcs, vec!["P0401", "P0403"]);
    let _ = std::fs::remove_file(&dtc_path);
}

#[test]
fn auto_loaded_sidecar_when_conventional_path_exists() {
    // Use a private temp-dir copy of the post-delete fixture so this
    // test doesn't race with `r19_skipped_when_no_dtc_sidecar_provided`
    // over the shared on-disk fixture sidecar.
    let dir = std::env::temp_dir().join("ecu_shenanigans_test_v4_autoload");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let src = fixture("vcds_amf_post_delete.csv");
    let csv = dir.join("vcds_amf_test_autoload.csv");
    std::fs::copy(&src, &csv).unwrap();
    let sidecar = sidecar_path_for(&csv);
    {
        let mut f = std::fs::File::create(&sidecar).unwrap();
        f.write_all(b"P0401\n").unwrap();
    }
    let log = parse_vcds_csv(&csv).unwrap();
    assert_eq!(log.dtcs, vec!["P0401"]);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn r19_skipped_when_no_dtc_sidecar_provided() {
    // Acceptance #7: missing --dtc → R19 SKIPPED.
    // Use a private temp-dir copy with no sidecar so this is hermetic.
    let dir = std::env::temp_dir().join("ecu_shenanigans_test_v4_no_sidecar");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let src = fixture("vcds_amf_post_delete.csv");
    let csv = dir.join("vcds_amf_test_no_sidecar.csv");
    std::fs::copy(&src, &csv).unwrap();
    // Confirm there is no sidecar in the temp dir.
    let sidecar = sidecar_path_for(&csv);
    assert!(!sidecar.exists());

    let log = parse_vcds_csv(&csv).unwrap();
    assert!(log.dtcs.is_empty());
    let df = resample_to_uniform(&log, DEFAULT_RATE_HZ);
    let result = analyse(df, log);
    let r19: Vec<_> = result.findings.iter().filter(|f| f.rule_id == "R19").collect();
    assert!(!r19.is_empty(), "R19 must produce a SKIPPED finding when no DTC scan provided");
    assert!(r19.iter().all(|f| f.skipped));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn r19_fires_when_sidecar_contains_p0401() {
    let dir = std::env::temp_dir();
    let dtc_path = dir.join("ecu_shenanigans_test_v4_p0401.dtc.txt");
    {
        let mut f = std::fs::File::create(&dtc_path).unwrap();
        f.write_all(b"P0401\nP0403\n").unwrap();
    }
    let log = parse_vcds_csv_with_dtc(fixture("vcds_amf_post_delete.csv"), &dtc_path).unwrap();
    let df = resample_to_uniform(&log, DEFAULT_RATE_HZ);
    let result = analyse(df, log);
    let r19: Vec<_> = result.findings.iter()
        .filter(|f| f.rule_id == "R19" && !f.skipped).collect();
    assert!(!r19.is_empty(), "R19 must fire when P0401 / P0403 in sidecar");
    let first = r19[0];
    assert_eq!(first.severity, ecu_shenanigans::rules::base::Severity::Warn);
    let _ = std::fs::remove_file(&dtc_path);
}
