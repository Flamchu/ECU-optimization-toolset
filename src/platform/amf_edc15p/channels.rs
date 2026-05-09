//! Canonical channel registry for AMF / EDC15P+ via VCDS (spec §5, v4).
//!
//! Every downstream module refers to channels by the snake_case names
//! below. The VCDS canonicalizer translates `NNN-K` headers into these
//! names; channels not in this registry are not implemented.
//!
//! Modelled-not-measured channels (lambda, EGT) live in helpers that
//! the rules call explicitly — they are not part of this registry.
//!
//! DTCs are NOT a channel — they come from a separate VCDS DTC scan
//! (sidecar file, `<base>.dtc.txt`) and are stored as `Vec<String>` on
//! the parsed log. See [`crate::ingest::dtc`].

/// One canonical channel definition.
#[derive(Debug, Clone, Copy)]
pub struct Channel {
    /// Canonical snake_case identifier used in code.
    pub name: &'static str,
    /// `NNN-K` group-field source(s) — pipe-separated when ambiguous.
    pub source: &'static str,
    /// Engineering unit string for display only.
    pub unit: &'static str,
    /// One-line note about the channel.
    pub description: &'static str,
}

/// Complete channel registry for AMF / EDC15P+ (v4).
pub const CHANNELS: &[Channel] = &[
    Channel { name: "rpm",            source: "001-1|003-1|008-1|011-1|020-1", unit: "rpm",       description: "engine speed (any group)" },
    Channel { name: "iq_actual",      source: "001-2",                          unit: "mg/stroke", description: "idle/cruise IQ — NOT WOT IQ" },
    Channel { name: "iq_requested",   source: "008-2",                          unit: "mg/stroke", description: "WOT IQ request — the number that matters" },
    Channel { name: "iq_limit_rpm",   source: "008-3",                          unit: "mg/stroke", description: "RPM-based fuel limit" },
    Channel { name: "iq_limit_maf",   source: "008-4",                          unit: "mg/stroke", description: "smoke-limiter cap" },
    Channel { name: "coolant_c",      source: "001-4|004-3",                    unit: "C",         description: "coolant temp" },
    Channel { name: "battery_v",      source: "004-2",                          unit: "V",         description: "battery voltage" },
    Channel { name: "maf_actual",     source: "003-3",                          unit: "mg/stroke", description: "measured airflow — fueling input" },
    Channel { name: "maf_spec",       source: "003-2",                          unit: "mg/stroke", description: "EGR closed-loop target" },
    Channel { name: "egr_duty",       source: "003-4",                          unit: "%",         description: "EGR valve duty" },
    Channel { name: "boost_spec",     source: "011-2",                          unit: "mbar abs",  description: "LDRXN output" },
    Channel { name: "boost_actual",   source: "011-3",                          unit: "mbar abs",  description: "PID-controlled actual" },
    Channel { name: "n75_duty",       source: "011-4",                          unit: "%",         description: "boost actuator drive" },
    Channel { name: "atm_pressure",   source: "010-2",                          unit: "mbar abs",  description: "ambient — capture key-on/engine-off" },
    Channel { name: "pedal_pct",      source: "010-4",                          unit: "%",         description: "driver-wish accelerator pedal % (G79). Group 010 field 4 is the canonical EDC15P+ TDI position. Used for WOT detection (≥ pedal_wot_pct) and the R17 cruise filter." },
    Channel { name: "map_abs_010",    source: "010-3",                          unit: "mbar abs",  description: "MAP actual on group 010 field 3 (PD-TDI label convention). NOT throttle — TDI has no TPS. The anti-shudder valve diagnostic lives in basic-settings, not in measuring blocks." },
    Channel { name: "soi_actual",     source: "020-2",                          unit: "deg BTDC",  description: "logged start-of-injection" },
    Channel { name: "map_abs",        source: "020-3",                          unit: "mbar abs",  description: "MAP cross-check vs 011-3" },
    Channel { name: "load_pct",       source: "020-4",                          unit: "%",         description: "engine load" },
    Channel { name: "torque_request", source: "015-2",                          unit: "Nm",        description: "from driver wish via TL" },
    Channel { name: "torque_actual",  source: "015-3",                          unit: "Nm",        description: "modelled actual" },
    Channel { name: "srcv_cyl1",      source: "013-1",                          unit: "mg/stroke", description: "smooth-running cyl 1" },
    Channel { name: "srcv_cyl2",      source: "013-2",                          unit: "mg/stroke", description: "smooth-running cyl 2" },
    Channel { name: "srcv_cyl3",      source: "013-3",                          unit: "mg/stroke", description: "smooth-running cyl 3 (AMF is 3-cyl)" },
    Channel { name: "fuel_temp_c",    source: "013-4|015-?",                    unit: "C",         description: "fuel temp on firmwares that expose it" },
    Channel { name: "vehicle_speed",  source: "005-3",                          unit: "km/h",      description: "VSS for cruise / idle distinction (group 005, field 3 — corrected from misnumbered 004)" },
    Channel { name: "coolant_demand_c", source: "007-?",                        unit: "C",         description: "coolant temperature setpoint (where exposed)" },
];

/// Channels we explicitly do not log (no factory sensor / not on KW1281).
/// Helpful for surfacing friendly errors instead of silent missing keys.
pub const NOT_LOGGED: &[&str] = &[
    "rail_pressure",
    "wideband_lambda",
    "egt_actual",
    "injector_duty",
];

/// Required minimum groups for any pull analysis to run.
pub const MIN_REQUIRED_GROUPS: &[&str] = &["003", "008", "011"];

/// Look up a channel definition by its canonical name.
pub fn channel(name: &str) -> Option<&'static Channel> {
    CHANNELS.iter().find(|c| c.name == name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rpm_is_registered() {
        let c = channel("rpm").expect("rpm channel must exist");
        assert_eq!(c.unit, "rpm");
    }

    #[test]
    fn unknown_channel_returns_none() {
        assert!(channel("definitely_not_a_channel").is_none());
    }

    #[test]
    fn min_groups_are_unique() {
        let mut groups: Vec<_> = MIN_REQUIRED_GROUPS.to_vec();
        groups.sort_unstable();
        groups.dedup();
        assert_eq!(groups.len(), MIN_REQUIRED_GROUPS.len());
    }

    #[test]
    fn channel_names_are_unique() {
        let mut seen: Vec<&str> = CHANNELS.iter().map(|c| c.name).collect();
        seen.sort_unstable();
        let original = seen.len();
        seen.dedup();
        assert_eq!(original, seen.len(), "duplicate channel name in registry");
    }

    #[test]
    fn vehicle_speed_sourced_from_group_005() {
        // v4 fix K: VSS lives in group 005 on PD-family EDC15P+, not 004.
        let vss = channel("vehicle_speed").expect("vehicle_speed must exist");
        assert!(vss.source.starts_with("005-"),
            "vehicle_speed source must be group 005, got: {}", vss.source);
    }

    #[test]
    fn pedal_pct_lives_at_group_010_field_4() {
        // pedal_pct is the canonical driver-wish channel and lives at
        // group 010 field 4 (G79 accelerator pedal sensor) on EDC15P+ TDI.
        let p = channel("pedal_pct").expect("pedal_pct must be registered");
        assert_eq!(p.source, "010-4");
        assert!(!p.source.contains("002"),
            "pedal_pct must not reference group 002 (idle-speed group, not pedal %)");
    }

    #[test]
    fn no_tps_pct_channel_exists() {
        // TDI has no throttle position sensor; the anti-shudder valve
        // diagnostic lives in basic-settings, not in measuring blocks.
        // Group 010 field 3 carries MAP actual on PD-TDI labels.
        assert!(channel("tps_pct").is_none(),
            "tps_pct must not be registered — TDI has no TPS");
        let m = channel("map_abs_010").expect("map_abs_010 must replace the old tps_pct entry");
        assert_eq!(m.source, "010-3");
        assert!(m.description.contains("anti-shudder"));
        assert!(m.description.contains("NOT throttle"));
    }

    #[test]
    fn dtc_codes_channel_is_removed() {
        // v4 fix J: DTCs are no longer encoded as a synthetic float channel.
        // They live in a sidecar file parsed by ingest::dtc.
        assert!(channel("dtc_codes").is_none());
    }
}
