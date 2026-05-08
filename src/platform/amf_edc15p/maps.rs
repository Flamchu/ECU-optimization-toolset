//! EDC15P+ map registry — symbolic only; the tool never reads/writes the
//! `.bin`.
//!
//! Names follow the public EDC15P+ damos / WinOLS / VAGEDCSuite tradition
//! so a user can paste delta suggestions directly into a tuning tool.

/// One symbolic map definition.
#[derive(Debug, Clone, Copy)]
pub struct MapDef {
    /// Canonical short id used in recommendations.
    pub name: &'static str,
    /// Common German alias from the EDC15P+ damos tradition.
    pub german_alias: &'static str,
    /// X axis identifier (e.g. `"rpm"`).
    pub x_axis: &'static str,
    /// Y axis identifier (e.g. `"iq_mg"`).
    pub y_axis: &'static str,
    /// Cell unit string for display only.
    pub cell_unit: &'static str,
    /// Typical map dimensions (e.g. `"16x10"`).
    pub typical_dim: &'static str,
    /// One-line note about the map and its sane Stage 1 delta.
    pub description: &'static str,
}

/// Complete EDC15P+ map registry.
pub const MAPS: &[MapDef] = &[
    MapDef {
        name: "LDRXN",
        german_alias: "Ladedruck-Sollwert",
        x_axis: "rpm", y_axis: "iq_mg",
        cell_unit: "mbar abs", typical_dim: "16x10",
        description: "Boost target. Sane Stage 1 Δ: +100..+200 mbar in 2000-3500 rpm band, taper to stock by 4000 rpm.",
    },
    MapDef {
        name: "LDOLLR",
        german_alias: "LDR-Sollwertbegrenzung (LDRPMX)",
        x_axis: "rpm", y_axis: "atm_mbar",
        cell_unit: "mbar abs", typical_dim: "16x10",
        description: "Boost limiter (max permitted absolute boost). Cap at 2150 mbar at sea level; preserve altitude derate.",
    },
    MapDef {
        name: "SVBL",
        german_alias: "Ladedruck-Begrenzung absolut",
        x_axis: "scalar", y_axis: "scalar",
        cell_unit: "mbar abs", typical_dim: "1x1",
        description: "Overboost cut (Single Value Boost Limit). LEAVE STOCK — last line of defence.",
    },
    MapDef {
        name: "Driver_Wish",
        german_alias: "Fahrerwunsch",
        x_axis: "pedal_pct", y_axis: "rpm",
        cell_unit: "mg/stroke", typical_dim: "8x16",
        description: "Pedal-to-IQ request. Raise WOT column by +6..+8 mg in 1800-3500 rpm band.",
    },
    MapDef {
        name: "Smoke_IQ_by_MAF",
        german_alias: "Begrenzungsmenge (MAF)",
        x_axis: "maf_mg", y_axis: "rpm",
        cell_unit: "mg/stroke", typical_dim: "13x16",
        description: "Smoke limiter, MAF axis. Re-scale axis & enforce λ ≥ 1.20 everywhere.",
    },
    MapDef {
        name: "Smoke_IQ_by_MAP",
        german_alias: "Begrenzungsmenge (MAP)",
        x_axis: "boost_mbar", y_axis: "rpm",
        cell_unit: "mg/stroke", typical_dim: "11x16",
        description: "Smoke limiter, MAP axis (active on AMF, switch byte = 257). Same lambda discipline.",
    },
    MapDef {
        name: "Torque_Limiter",
        german_alias: "Drehmomentbegrenzer",
        x_axis: "rpm", y_axis: "atm_mbar",
        cell_unit: "Nm", typical_dim: "20x3",
        description: "Modelled-torque cap. Clamp peak to 240 Nm equivalent — clutch-protective.",
    },
    MapDef {
        name: "MLHFM",
        german_alias: "Luftmassenmesser-Kennlinie",
        x_axis: "sensor_v_or_raw", y_axis: "_",
        cell_unit: "kg/h", typical_dim: "256pts",
        description: "MAF linearisation. LEAVE STOCK unless MAF is replaced; flag if log-derived MAF deviates >10 % from spec.",
    },
    MapDef {
        name: "SOI",
        german_alias: "Spritzbeginn (10 maps by coolant temp)",
        x_axis: "rpm", y_axis: "iq_mg",
        cell_unit: "deg BTDC", typical_dim: "10x10 (cold) / 10x16 (hot)",
        description: "Start-of-injection. Sane Δ: +1.5..+2.5° at 4000 rpm column only; NEVER exceed 26° BTDC.",
    },
    MapDef {
        name: "Duration",
        german_alias: "Einspritzdauer (6 maps by SOI band)",
        x_axis: "rpm", y_axis: "iq_mg",
        cell_unit: "deg crank", typical_dim: "10x10 / 16x15",
        description: "Injection duration. Extend X-axis from 50 to 52 mg if extending IQ.",
    },
    MapDef {
        name: "Pilot",
        german_alias: "Voreinspritzmenge / -zeit",
        x_axis: "rpm", y_axis: "iq_mg",
        cell_unit: "mg/stroke or deg", typical_dim: "10x10",
        description: "Pilot injection quantity & timing. LEAVE STOCK for sane Stage 1.",
    },
    MapDef {
        name: "N75_duty",
        german_alias: "Ladedruckregler-Tastverhältnis",
        x_axis: "rpm", y_axis: "spec_actual_diff_or_iq",
        cell_unit: "% DC", typical_dim: "10x16",
        description: "N75 base duty / PID. LEAVE STOCK unless steady-state error > 150 mbar.",
    },
    MapDef {
        name: "Lambda_limiter",
        german_alias: "Lambdawunsch / Rauchbegrenzung",
        x_axis: "maf_mg", y_axis: "rpm",
        cell_unit: "lambda", typical_dim: "13x16",
        description: "Floor cells at λ = 1.20.",
    },
    MapDef {
        name: "Atmospheric_correction",
        german_alias: "Höhenkorrektur LDR",
        x_axis: "atm_mbar", y_axis: "_",
        cell_unit: "mbar Δ", typical_dim: "10x1",
        description: "Altitude derate for boost. LEAVE STOCK.",
    },
    MapDef {
        name: "EGT_model",
        german_alias: "Abgastemperatur-Modell",
        x_axis: "rpm_iq_maf", y_axis: "_",
        cell_unit: "C", typical_dim: "varies",
        description: "EGT model / fuel cut on temp (where present). DO NOT RAISE — used as backstop.",
    },
];

/// Look up a map by its canonical short id.
pub fn get_map(name: &str) -> Option<&'static MapDef> {
    MAPS.iter().find(|m| m.name == name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_has_fifteen_maps() {
        assert_eq!(MAPS.len(), 15);
    }

    #[test]
    fn known_maps_are_findable() {
        assert!(get_map("LDRXN").is_some());
        assert!(get_map("SVBL").is_some());
    }

    #[test]
    fn unknown_map_is_none() {
        assert!(get_map("NOT_A_MAP").is_none());
    }
}
