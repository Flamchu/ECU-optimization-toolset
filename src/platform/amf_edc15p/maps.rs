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
        description: "Smoke limiter, MAF axis. Re-scale axis & enforce λ ≥ 1.05 everywhere.",
    },
    MapDef {
        name: "Smoke_IQ_by_MAP",
        german_alias: "Begrenzungsmenge (MAP)",
        x_axis: "boost_mbar", y_axis: "rpm",
        cell_unit: "mg/stroke", typical_dim: "11x16",
        description: "Smoke limiter, MAP axis (parallel slice, switch byte selects). Same λ ≥ 1.05 discipline.",
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
        description: "Floor cells at λ = 1.05.",
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
    // ---- v3/v4 EGR-delete-specific symbolic maps ------------------------
    MapDef {
        name: "AGR_arwMEAB0KL",
        german_alias: "Abgasrückführung Tastverhältnis (Bank A)",
        x_axis: "rpm", y_axis: "iq_mg",
        cell_unit: "% duty", typical_dim: "13x16",
        description: "EGR-duty map bank A. v4 mandate: zero all cells.",
    },
    MapDef {
        name: "AGR_arwMEAB1KL",
        german_alias: "Abgasrückführung Tastverhältnis (Bank B)",
        x_axis: "rpm", y_axis: "iq_mg",
        cell_unit: "% duty", typical_dim: "13x16",
        description: "EGR-duty map bank B (paired in DAMOS even on single-actuator PD ECUs). \
                      v4 mandate: zero all cells — symmetry with bank A.",
    },
    MapDef {
        name: "arwMLGRDKF",
        german_alias: "Sollluftmasse / EGR target air mass",
        x_axis: "rpm", y_axis: "iq_mg",
        cell_unit: "mg/stroke", typical_dim: "16x10",
        description: "Spec-MAF / expected air mass. v4 mandate: fill ≥850 mg/stroke (Strategy B).",
    },
    MapDef {
        name: "DTC_thresholds",
        german_alias: "DTC-Grenzwerte",
        x_axis: "dtc_id", y_axis: "_",
        cell_unit: "mg/s · ms", typical_dim: "varies",
        description: "DTC plausibility thresholds and time-debounce for P0401..P0406.",
    },
    MapDef {
        name: "MAF_MAP_smoke_switch",
        german_alias: "Rauchbegrenzung-Quelle (MAF/MAP)",
        x_axis: "scalar", y_axis: "scalar",
        cell_unit: "byte", typical_dim: "1x1",
        description: "Smoke-limiter source switch. 0x00 = MAF-based (v3 LEAVE STOCK).",
    },
    MapDef {
        name: "Idle_fuel",
        german_alias: "Leerlauf-Mengenkennfeld (Slice)",
        x_axis: "rpm", y_axis: "iq_mg",
        cell_unit: "mg/stroke", typical_dim: "varies",
        description: "Idle fuelling slice. CONDITIONAL: trim only if R21 idle-stability fires.",
    },
    MapDef {
        name: "SOI_warm_cruise",
        german_alias: "Spritzbeginn warm (Cruise-Band)",
        x_axis: "rpm", y_axis: "iq_mg",
        cell_unit: "deg BTDC", typical_dim: "10x16",
        description: "Warm-cruise SOI band (1500-2500 rpm × 5-15 mg). −1.0° NVH retard.",
    },
    // ---- Driveability + thermal symbolic maps ---------------------------
    MapDef {
        name: "Driver_Wish_low_pedal",
        german_alias: "Fahrerwunsch (Pedal 1..25 %)",
        x_axis: "pedal_pct", y_axis: "rpm",
        cell_unit: "mg/stroke", typical_dim: "13x16 with 5 parallel banks (per cowFUN_DSV)",
        description: "View of the Driver_Wish (Fahrerwunsch / mrwFVH_KF) restricted \
                      to the 1..25 % pedal column band. Edits here flatten the off-idle \
                      slope; mid- and high-pedal cells are not modified. EDC15P+ carries \
                      5 parallel banks (banks 2/3/5 manual, 1/4 automatic per cowFUN_DSV \
                      codeblock detail); apply the flatten identically to all 5 banks to \
                      remain coding-state-invariant.",
    },
    MapDef {
        name: "Fan_thresholds",
        german_alias: "Lüfter Schwellenwerte (firmware-dependent)",
        x_axis: "stage", y_axis: "_",
        cell_unit: "°C", typical_dim: "stage-1 on/off, stage-2 on/off",
        description: "Cooling-fan stage on/off thresholds. Symbolic only — actual DAMOS \
                      naming is firmware-dependent on EDC15P+; locate against the AMF binary.",
    },
    MapDef {
        name: "Fan_run_on",
        german_alias: "Lüfter-Nachlauf (firmware-dependent)",
        x_axis: "scalar", y_axis: "_",
        cell_unit: "s", typical_dim: "scalar (or short LUT indexed by T_coolant at key-off)",
        description: "Fan run-on time after key-off. Capped at the battery-protective ceiling.",
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
    fn registry_has_25_maps() {
        // 22 platform/EGR maps + 3 driveability/thermal symbolic maps.
        assert_eq!(MAPS.len(), 25);
    }

    #[test]
    fn egr_maps_are_present_both_banks() {
        for name in [
            "AGR_arwMEAB0KL",
            "AGR_arwMEAB1KL",
            "arwMLGRDKF",
            "DTC_thresholds",
            "MAF_MAP_smoke_switch",
            "Idle_fuel",
            "SOI_warm_cruise",
        ] {
            assert!(get_map(name).is_some(), "EGR map {name} must be in registry");
        }
    }

    #[test]
    fn driveability_and_thermal_maps_are_present() {
        for name in [
            "Driver_Wish_low_pedal",
            "Fan_thresholds",
            "Fan_run_on",
        ] {
            assert!(get_map(name).is_some(),
                "driveability/thermal map {name} must be in registry");
        }
    }

    #[test]
    fn map_ids_are_unique() {
        let mut ids: Vec<&str> = MAPS.iter().map(|m| m.name).collect();
        ids.sort_unstable();
        let original_len = ids.len();
        ids.dedup();
        assert_eq!(original_len, ids.len(), "duplicate map id in registry");
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
