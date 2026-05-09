//! Cross-cutting invariants — checks that don't fit any single
//! module's unit tests.

use std::collections::BTreeMap;
use std::path::Path;

use ecu_shenanigans::ingest::{canonical_name, VcdsLog};
use ecu_shenanigans::platform::amf_edc15p::channels::channel;
use ecu_shenanigans::platform::amf_edc15p::default_deltas::DEFAULT_DELTAS;
use ecu_shenanigans::platform::amf_edc15p::envelope::CAPS;
use ecu_shenanigans::util::resample_to_uniform;

#[test]
fn cargo_version_pinned() {
    assert_eq!(ecu_shenanigans::VERSION, "6.0.0");
}

#[test]
fn cargo_binary_name_unchanged() {
    let cargo = std::fs::read_to_string("Cargo.toml").expect("Cargo.toml");
    assert!(cargo.contains("name = \"ecu-shenanigans\""));
    assert!(cargo.contains("version = \"6.0.0\""));
}

/// Build the wrong-turbo identifier at runtime so this very file does
/// not trip the search. The misidentification string must not appear
/// anywhere in source / docs / tests; the test itself references it
/// only via run-time concatenation to stay clean.
fn wrong_turbo_token() -> String {
    let mut s = String::from("KP");
    s.push_str("35");
    s
}

/// Likewise for the prefix.
fn wrong_oem_token() -> String {
    let mut s = String::from("K");
    s.push_str("KK"); // form from "K" + "KK" so this string also doesn't appear literally
    s
}

#[test]
fn no_wrong_turbo_string_anywhere_in_source_or_docs() {
    let needle = wrong_turbo_token();
    let mut hits: Vec<String> = Vec::new();
    walk("src", &needle, &mut hits);
    walk("tests", &needle, &mut hits);
    walk_file(Path::new("README.md"), &needle, &mut hits);
    walk_file(Path::new("docs/rules.md"), &needle, &mut hits);
    walk_file(Path::new("docs/platform_amf.md"), &needle, &mut hits);
    walk_file(Path::new("docs/specification.md"), &needle, &mut hits);
    walk_file(Path::new("docs/damos_pointers.md"), &needle, &mut hits);
    if !hits.is_empty() {
        panic!("Wrong-turbo string found in:\n{}", hits.join("\n"));
    }
}

#[test]
fn no_wrong_oem_string_in_source() {
    // Don't sweep README.md — it deliberately references the wrong OEM
    // alongside the correct one in a "NOT this — instead that" sentence.
    let needle = wrong_oem_token();
    let mut hits: Vec<String> = Vec::new();
    walk("src", &needle, &mut hits);
    if !hits.is_empty() {
        panic!("Wrong-OEM string found in src:\n{}", hits.join("\n"));
    }
}

fn walk(dir: &str, needle: &str, hits: &mut Vec<String>) {
    let p = Path::new(dir);
    if !p.exists() { return; }
    for entry in std::fs::read_dir(p).unwrap().flatten() {
        let path = entry.path();
        if path.is_dir() {
            let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
            if name == "target" || name == "dev" || name.starts_with('.') { continue; }
            walk(path.to_str().unwrap(), needle, hits);
        } else {
            walk_file(&path, needle, hits);
        }
    }
}

fn walk_file(path: &Path, needle: &str, hits: &mut Vec<String>) {
    if !path.exists() { return; }
    let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("");
    if !matches!(ext, "rs" | "md" | "toml") { return; }
    let Ok(text) = std::fs::read_to_string(path) else { return };
    for (i, line) in text.lines().enumerate() {
        let upper = line.to_ascii_uppercase();
        if upper.contains(needle) {
            hits.push(format!("{}:{}: {}", path.display(), i + 1, line));
        }
    }
}

#[test]
fn vehicle_speed_canonicalizer_routes_005_3() {
    assert_eq!(canonical_name("005-3"), Some("vehicle_speed"));
}

#[test]
fn vehicle_speed_channel_lists_group_005() {
    let v = channel("vehicle_speed").expect("vehicle_speed channel must exist");
    assert!(v.source.starts_with("005-"),
        "vehicle_speed source must be group 005, got: {}", v.source);
}

#[test]
fn pedal_pct_lives_at_g79_position() {
    // pedal_pct is the canonical driver-wish channel and lives at group
    // 010 field 4 (G79 accelerator pedal sensor) on EDC15P+ TDI. The
    // diesel has no TPS, so no tps_pct channel exists either.
    let pedal = channel("pedal_pct").expect("pedal_pct missing");
    assert_eq!(pedal.source, "010-4");
    assert!(channel("tps_pct").is_none(),
        "tps_pct must not be registered — TDI has no throttle position sensor");
    let map_alt = channel("map_abs_010")
        .expect("map_abs_010 must replace tps_pct on group 010 field 3");
    assert!(map_alt.description.to_ascii_lowercase().contains("anti-shudder"));
}

#[test]
fn dtc_codes_float_channel_is_gone() {
    assert!(channel("dtc_codes").is_none());
}

#[test]
fn resampler_locf_holds_egr_duty_across_gap() {
    // v4 acceptance #3: with samples 0.0 → 30.0 separated by a 1-second
    // gap, the resampled middle samples must read 0.0 (LOCF), NOT 15.0.
    let mut data: BTreeMap<String, Vec<f64>> = BTreeMap::new();
    data.insert("egr_duty".to_string(), vec![0.0, 30.0]);
    let log = VcdsLog {
        source_file: std::path::PathBuf::from("synth.csv"),
        time: vec![0.0, 1.0],
        data,
        groups: Default::default(),
        field_names: Default::default(),
        units: Default::default(),
        unmapped_columns: Vec::new(),
        warnings: Vec::new(),
        median_sample_dt_ms: 1000.0,
        dtcs: Vec::new(),
    };
    let df = resample_to_uniform(&log, 5.0); // 5 Hz → 200 ms grid
    let duty = df.data.get("egr_duty").expect("egr_duty resampled");
    assert_eq!(duty[0], 0.0);
    let last = duty[duty.len() - 1];
    assert_eq!(last, 30.0);
    for &v in &duty[1..duty.len() - 1] {
        assert_eq!(v, 0.0,
            "LOCF: gap samples must hold the last observation (0.0), not interpolate");
    }
}

#[test]
fn lambda_cross_link_default_deltas_to_caps() {
    // v4 acceptance #4: smoke rows reference the same lambda value baked
    // into CAPS — we already cross-link via a const assert in default_deltas.rs,
    // and this test confirms the runtime substring matches too.
    let smoke_rows: Vec<_> = DEFAULT_DELTAS.iter()
        .filter(|d| d.map_name.starts_with("Smoke_IQ"))
        .collect();
    assert!(!smoke_rows.is_empty(), "smoke rows must exist");
    let token = format!("{}", CAPS.lambda_floor);
    for row in &smoke_rows {
        assert!(row.note.contains(&token),
            "smoke row {} note must mention λ ≥ {}", row.map_name, token);
    }
}

#[test]
fn caps_lambda_floor_is_canonical_v4_value() {
    assert!((CAPS.lambda_floor - 1.05).abs() < f64::EPSILON);
}
