//! Channel-source audit — pins the canonical VCDS group/field
//! assignments for the driver-wish pedal and the PD-TDI MAP-actual
//! position. Standing regression: a refactor that maps `pedal_pct` back
//! to group 002 (idle-speed group) or re-introduces `tps_pct` (which
//! does not exist on a TDI) is caught here.

use ecu_shenanigans::ingest::canonical_name;
use ecu_shenanigans::platform::amf_edc15p::channels::channel;

#[test]
fn a16_pedal_pct_source() {
    let p = channel("pedal_pct").expect("pedal_pct must be registered");
    assert_eq!(p.source, "010-4",
        "pedal_pct must come from group 010 field 4 (G79), not group 002");
    assert!(!p.source.contains("002"),
        "pedal_pct source must not reference group 002 (that's idle-speed, not pedal %)");
    // Canonicalizer routes 010-4 to pedal_pct.
    assert_eq!(canonical_name("010-4"), Some("pedal_pct"));
}

#[test]
fn a17_tps_pct_semantics() {
    assert!(channel("tps_pct").is_none(),
        "tps_pct must be removed — TDI has no TPS, this channel was mis-mapped");
    let m = channel("map_abs_010").expect("map_abs_010 must replace tps_pct on group 010 field 3");
    assert_eq!(m.source, "010-3");
    assert!(m.description.contains("anti-shudder"));
    assert!(m.description.contains("NOT throttle"));
    // Canonicalizer routes 010-3 to map_abs_010 (no longer to a TPS-named channel).
    assert_eq!(canonical_name("010-3"), Some("map_abs_010"));
}

#[test]
fn no_canonical_mapping_to_tps_pct() {
    // Defence-in-depth: even if someone re-adds the channel to the
    // registry, the canonicalizer must never route a group/field to it.
    for nnn_k in [
        "001-1", "001-2", "001-3", "001-4",
        "003-1", "003-2", "003-3", "003-4",
        "004-1", "004-2", "004-3", "004-4",
        "005-1", "005-2", "005-3",
        "008-1", "008-2", "008-3", "008-4",
        "010-1", "010-2", "010-3", "010-4",
        "011-1", "011-2", "011-3", "011-4",
        "013-1", "013-2", "013-3", "013-4",
        "015-1", "015-2", "015-3",
        "020-1", "020-2", "020-3", "020-4",
        "031-2", "031-3",
    ] {
        if let Some(name) = canonical_name(nnn_k) {
            assert_ne!(name, "tps_pct",
                "{nnn_k} must not canonicalise to tps_pct");
        }
    }
}
