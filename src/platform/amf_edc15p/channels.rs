//! Canonical channel registry for AMF / EDC15P+ via VCDS (spec §8).
//!
//! Every downstream module refers to channels by the snake_case names
//! below. The VCDS canonicalizer translates `NNN-K` headers into these
//! names; channels not in this registry are not implemented.
//!
//! Modelled-not-measured channels (lambda, EGT) live in helpers that
//! the rules call explicitly — they are not part of this registry.

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

/// Complete channel registry for AMF / EDC15P+.
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
    Channel { name: "tps_pct",        source: "010-3",                          unit: "%",         description: "throttle/pedal proxy" },
    Channel { name: "soi_actual",     source: "020-2",                          unit: "deg BTDC",  description: "logged start-of-injection" },
    Channel { name: "map_abs",        source: "020-3",                          unit: "mbar abs",  description: "MAP cross-check vs 011-3" },
    Channel { name: "load_pct",       source: "020-4",                          unit: "%",         description: "engine load" },
    Channel { name: "torque_request", source: "015-2",                          unit: "Nm",        description: "from driver wish via TL" },
    Channel { name: "torque_actual",  source: "015-3",                          unit: "Nm",        description: "modelled actual" },
    Channel { name: "srcv_cyl1",      source: "013-1",                          unit: "mg/stroke", description: "smooth-running cyl 1" },
    Channel { name: "srcv_cyl2",      source: "013-2",                          unit: "mg/stroke", description: "smooth-running cyl 2" },
    Channel { name: "srcv_cyl3",      source: "013-3",                          unit: "mg/stroke", description: "smooth-running cyl 3 (AMF is 3-cyl)" },
    Channel { name: "fuel_temp_c",    source: "013-?",                          unit: "C",         description: "fuel temp on firmwares that expose it" },
];

/// Channels we explicitly do not log (no factory sensor / not on KW1281).
/// Helpful for surfacing friendly errors instead of silent missing keys.
pub const NOT_LOGGED: &[&str] = &[
    "rail_pressure",
    "wideband_lambda",
    "egt_actual",
    "injector_duty",
];

/// Required minimum groups for any pull analysis to run (spec §6.2).
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
}
