//! v3 EGR-delete acceptance tests against the on-disk pre/post-delete
//! fixtures. Mirrors spec §13 acceptance criteria #1, #4, #5, #6.

use std::path::PathBuf;

use ecu_shenanigans::ingest::parse_vcds_csv;
use ecu_shenanigans::recommend::{recommend, render_markdown, Status};
use ecu_shenanigans::rules::analyse;
use ecu_shenanigans::util::resample_to_uniform;
use ecu_shenanigans::util::timebase::DEFAULT_RATE_HZ;
use ecu_shenanigans::validate::{validate_egr_delete, CheckStatus};

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests").join("fixtures").join(name)
}

#[test]
fn post_delete_validation_passes() {
    let log = parse_vcds_csv(fixture("vcds_amf_post_delete.csv")).unwrap();
    let report = validate_egr_delete(&log);
    let failed: Vec<&str> = report.items.iter()
        .filter(|i| i.status == CheckStatus::Fail)
        .map(|i| i.title.as_str())
        .collect();
    assert!(report.pass(),
        "post-delete fixture must pass validation. Failures: {:?}", failed);
}

#[test]
fn pre_delete_validation_fails_egr_duty() {
    let log = parse_vcds_csv(fixture("vcds_amf_pre_delete.csv")).unwrap();
    let report = validate_egr_delete(&log);
    assert!(!report.pass(), "pre-delete fixture must fail validation");

    let item1 = report.items.iter().find(|i| i.id == 1).unwrap();
    assert_eq!(item1.status, CheckStatus::Fail,
        "EGR duty at idle must FAIL on pre-delete log");
}

#[test]
fn report_contains_v3_required_sections() {
    let log = parse_vcds_csv(fixture("vcds_amf_post_delete.csv")).unwrap();
    let df = resample_to_uniform(&log, DEFAULT_RATE_HZ);
    let result = analyse(df, log);
    let recs = recommend(&result.findings);
    let md = render_markdown(&result, &recs, None);

    // Spec §13 acceptance #1: required sections.
    assert!(md.contains("EGR Delete Strategy"),
        "report must include the EGR Delete Strategy section");
    assert!(md.contains("Recommendation table"),
        "report must include the recommendation table");
    assert!(md.contains("AGR_arwMEAB0KL"),
        "EGR duty map must be in the Strategy table");
    assert!(md.contains("arwMLGRDKF"),
        "Spec-MAF map must be in the Strategy table");
}

#[test]
fn recommendation_table_has_v3_egr_rows() {
    // Spec §13 acceptance #2: deltas table includes EGR-duty (0%),
    // spec-MAF (≥850), DTC suppression, idle conditional, cruise SOI.
    let log = parse_vcds_csv(fixture("vcds_amf_post_delete.csv")).unwrap();
    let df = resample_to_uniform(&log, DEFAULT_RATE_HZ);
    let result = analyse(df, log);
    let recs = recommend(&result.findings);

    let names: Vec<&str> = recs.iter().map(|r| r.map_name.as_str()).collect();
    for required in &[
        "AGR_arwMEAB0KL", "arwMLGRDKF", "DTC_thresholds",
        "MAF_MAP_smoke_switch", "Idle_fuel", "SOI_warm_cruise",
    ] {
        assert!(names.contains(required),
            "recommendation table missing v3 row: {required}");
    }

    // EGR-duty + spec-MAF + DTC widening must be APPLY (unconditional).
    let egr = recs.iter().find(|r| r.map_name == "AGR_arwMEAB0KL").unwrap();
    assert_eq!(egr.status, Status::Apply);
    let spec = recs.iter().find(|r| r.map_name == "arwMLGRDKF").unwrap();
    assert_eq!(spec.status, Status::Apply);
    let dtc = recs.iter().find(|r| r.map_name == "DTC_thresholds").unwrap();
    assert_eq!(dtc.status, Status::Apply);

    // Switch must be SKIP (LEAVE STOCK).
    let switch = recs.iter().find(|r| r.map_name == "MAF_MAP_smoke_switch").unwrap();
    assert_eq!(switch.status, Status::Skip);
}

#[test]
fn pre_delete_log_fires_r16() {
    // Spec §13 acceptance #5: ecu-opt validate-egr-delete on pre-delete
    // log must exit non-zero (R16 critical: EGR duty observed > 0%).
    let log = parse_vcds_csv(fixture("vcds_amf_pre_delete.csv")).unwrap();
    let df = resample_to_uniform(&log, DEFAULT_RATE_HZ);
    let result = analyse(df, log);
    let r16: Vec<_> = result.findings.iter()
        .filter(|f| f.rule_id == "R16" && !f.skipped)
        .collect();
    assert!(!r16.is_empty(), "R16 must fire on pre-delete log");
    assert_eq!(r16[0].severity, ecu_shenanigans::rules::base::Severity::Critical);
}

#[test]
fn post_delete_log_does_not_fire_r16() {
    let log = parse_vcds_csv(fixture("vcds_amf_post_delete.csv")).unwrap();
    let df = resample_to_uniform(&log, DEFAULT_RATE_HZ);
    let result = analyse(df, log);
    let r16: Vec<_> = result.findings.iter()
        .filter(|f| f.rule_id == "R16" && !f.skipped)
        .collect();
    assert!(r16.is_empty(),
        "R16 must NOT fire on post-delete log (egr_duty ≤ tolerance)");
}

#[test]
fn validation_markdown_renders_for_post_delete() {
    let log = parse_vcds_csv(fixture("vcds_amf_post_delete.csv")).unwrap();
    let report = validate_egr_delete(&log);
    let md = report.to_markdown();
    assert!(md.contains("EGR Delete Validation Checklist"));
    assert!(md.contains("**Result: PASS**"));
}
