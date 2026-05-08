//! End-to-end test of the ingest → resample → analyse → recommend
//! pipeline against the on-disk VCDS fixtures.

use std::path::PathBuf;

use ecu_shenanigans::ingest::parse_vcds_csv;
use ecu_shenanigans::recommend::{recommend, render_markdown, write_report, Status};
use ecu_shenanigans::rules::analyse;
use ecu_shenanigans::util::resample_to_uniform;
use ecu_shenanigans::util::timebase::DEFAULT_RATE_HZ;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests").join("fixtures").join(name)
}

#[test]
fn full_pipeline_on_overboost_fixture_emits_markdown() {
    let log = parse_vcds_csv(fixture("vcds_amf_008_011.csv")).unwrap();
    let df = resample_to_uniform(&log, DEFAULT_RATE_HZ);
    let result = analyse(df, log);
    let recs = recommend(&result.findings);
    let md = render_markdown(&result, &recs);
    assert!(md.contains("ecu-shenanigans — Analysis report"));
    assert!(md.contains("ecu-shenanigans"));
    assert!(md.contains("Recommendation table"));
    // SVBL must always be present and skipped (leave stock).
    assert!(md.contains("`SVBL`"));
}

#[test]
fn full_pipeline_writes_report_file() {
    let tmp = std::env::temp_dir().join("ecu_shenanigans_test_out");
    let _ = std::fs::remove_dir_all(&tmp);
    let log = parse_vcds_csv(fixture("vcds_amf_001_003_011.csv")).unwrap();
    let df = resample_to_uniform(&log, DEFAULT_RATE_HZ);
    let result = analyse(df, log);
    let recs = recommend(&result.findings);
    let path = write_report(&result, &recs, &tmp).expect("write report");
    assert!(path.exists(), "report file written: {}", path.display());
    let body = std::fs::read_to_string(&path).expect("read back");
    assert!(body.contains("Analysis report"));
}

#[test]
fn svbl_is_always_in_recommendations() {
    let log = parse_vcds_csv(fixture("vcds_amf_001_003_011.csv")).unwrap();
    let df = resample_to_uniform(&log, DEFAULT_RATE_HZ);
    let result = analyse(df, log);
    let recs = recommend(&result.findings);
    let svbl = recs.iter().find(|r| r.map_name == "SVBL")
        .expect("SVBL row always emitted");
    assert_eq!(svbl.status, Status::Skip);
    assert_eq!(svbl.proposed_value_text, "leave stock");
}

#[test]
fn recommendation_count_matches_default_deltas_table() {
    use ecu_shenanigans::platform::amf_edc15p::default_deltas::DEFAULT_DELTAS;
    let log = parse_vcds_csv(fixture("vcds_amf_001_003_011.csv")).unwrap();
    let df = resample_to_uniform(&log, DEFAULT_RATE_HZ);
    let result = analyse(df, log);
    let recs = recommend(&result.findings);
    assert_eq!(recs.len(), DEFAULT_DELTAS.len());
}

#[test]
fn no_pull_log_renders_useful_message() {
    use ecu_shenanigans::ingest::VcdsLog;
    use ecu_shenanigans::util::ResampledLog;
    use std::collections::{BTreeMap, BTreeSet};
    let log = VcdsLog {
        source_file: PathBuf::from("synth.csv"),
        time: vec![],
        data: BTreeMap::new(),
        groups: BTreeSet::new(),
        field_names: Default::default(),
        units: Default::default(),
        unmapped_columns: Vec::new(),
        warnings: Vec::new(),
        median_sample_dt_ms: 0.0,
    };
    let df = ResampledLog { time: Vec::new(), data: BTreeMap::new() };
    let result = analyse(df, log);
    let recs = recommend(&result.findings);
    let md = render_markdown(&result, &recs);
    assert!(md.contains("No WOT pulls detected"));
}
