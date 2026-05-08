//! Default sane Stage 1 deltas per spec §4.4.
//!
//! Used by the recommendation engine when no user-supplied basemap is
//! available; each delta is then run through `envelope::clamp_*` to
//! confirm the resulting value stays inside the §5 envelope.

/// Kind of action a [`DefaultDelta`] represents.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeltaKind {
    /// Add a delta in mbar to the current value.
    DeltaMbar,
    /// Add a delta in mg/stroke.
    DeltaMg,
    /// Add a delta in degrees.
    DeltaDeg,
    /// Set the value to a fixed target.
    SetTo,
    /// Leave the map at stock.
    LeaveStock,
    /// Extend the axis end to a new endpoint.
    ExtendAxis,
    /// Clamp the peak of the surface to a fixed value (or via lambda model).
    ClampPeak,
}

/// One row in the default sane-deltas table.
#[derive(Debug, Clone, Copy)]
pub struct DefaultDelta {
    /// Symbolic map this delta targets.
    pub map_name: &'static str,
    /// Free-text cell selector (e.g. `"rpm 2000-3500 × IQ 40-50 mg"`).
    pub cell_selector: &'static str,
    /// Action kind.
    pub kind: DeltaKind,
    /// Delta / target / extend-to value, where applicable.
    pub value: Option<f64>,
    /// Rule ids that need to fire to APPLY this delta.
    pub rule_refs: &'static [&'static str],
    /// Note printed verbatim in the report rationale.
    pub note: &'static str,
}

/// Canonical default-delta table.
pub const DEFAULT_DELTAS: &[DefaultDelta] = &[
    DefaultDelta {
        map_name: "LDRXN",
        cell_selector: "rpm 2000-3500 × IQ 40-50 mg",
        kind: DeltaKind::DeltaMbar,
        value: Some(150.0),
        rule_refs: &["R02", "R03"],
        note: "Bounded to absolute ≤ 2150 mbar by clamp_boost_target.",
    },
    DefaultDelta {
        map_name: "LDRXN",
        cell_selector: "rpm 4000-4500 (taper)",
        kind: DeltaKind::DeltaMbar,
        value: Some(-50.0),
        rule_refs: &["R04"],
        note: "Hold at stock − 50 mbar to keep KP35 in efficiency island.",
    },
    DefaultDelta {
        map_name: "Driver_Wish",
        cell_selector: "pedal 100% × rpm 1800-3500",
        kind: DeltaKind::SetTo,
        value: Some(50.0),
        rule_refs: &["R07"],
        note: "Raise to 50 mg/stroke; bounded by clamp_iq.",
    },
    DefaultDelta {
        map_name: "Smoke_IQ_by_MAP",
        cell_selector: "boost 2000-2150 mbar × rpm 2000-3500",
        kind: DeltaKind::ClampPeak,
        value: None,
        rule_refs: &["R06"],
        note: "Enforce λ ≥ 1.20 by computed IQ cap from MAF model.",
    },
    DefaultDelta {
        map_name: "Smoke_IQ_by_MAF",
        cell_selector: "MAF 600-750 mg/str × rpm 2000-3500",
        kind: DeltaKind::ClampPeak,
        value: None,
        rule_refs: &["R06"],
        note: "Same λ ≥ 1.20 floor, in MAF space.",
    },
    DefaultDelta {
        map_name: "Torque_Limiter",
        cell_selector: "full surface",
        kind: DeltaKind::ClampPeak,
        value: Some(240.0),
        rule_refs: &["R08"],
        note: "Clamp peak modelled torque to 240 Nm — LUK SMF ceiling.",
    },
    DefaultDelta {
        map_name: "SOI",
        cell_selector: "rpm 3500-4500 × IQ 40-50 mg (warm map only)",
        kind: DeltaKind::DeltaDeg,
        value: Some(1.5),
        rule_refs: &["R09"],
        note: "+1.5° BTDC, capped at 26° absolute by clamp_soi.",
    },
    DefaultDelta {
        map_name: "Duration",
        cell_selector: "X-axis end",
        kind: DeltaKind::ExtendAxis,
        value: Some(52.0),
        rule_refs: &["R07"],
        note: "Extend axis 50 → 52 mg, proportional only.",
    },
    DefaultDelta {
        map_name: "SVBL",
        cell_selector: "scalar",
        kind: DeltaKind::LeaveStock,
        value: None,
        rule_refs: &[],
        note: "Last line of defence — never touch.",
    },
    DefaultDelta {
        map_name: "N75_duty",
        cell_selector: "full surface",
        kind: DeltaKind::LeaveStock,
        value: None,
        rule_refs: &["R01"],
        note: "Leave stock unless R01 fires repeatedly.",
    },
    DefaultDelta {
        map_name: "Pilot",
        cell_selector: "full surface",
        kind: DeltaKind::LeaveStock,
        value: None,
        rule_refs: &[],
        note: "Pilot is NVH, not power. Leave alone.",
    },
    DefaultDelta {
        map_name: "MLHFM",
        cell_selector: "full surface",
        kind: DeltaKind::LeaveStock,
        value: None,
        rule_refs: &["R05"],
        note: "Leave stock unless MAF replaced or drift > 10 %.",
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_default_has_a_known_map() {
        use crate::platform::amf_edc15p::maps::get_map;
        for d in DEFAULT_DELTAS {
            assert!(get_map(d.map_name).is_some(),
                "default delta references unknown map: {}", d.map_name);
        }
    }
}
