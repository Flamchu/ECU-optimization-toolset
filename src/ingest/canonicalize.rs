//! Translate VCDS `NNN-K` column ids into canonical channel names.
//!
//! This is the only place in the codebase where group-field ids become
//! channel names — every other module talks canonical names exclusively.
//!
//! Some channels appear in multiple groups (RPM is in 001/003/008/011/020).
//! When several groups carry the same canonical channel, the first present
//! wins; subsequent duplicates are kept under a `__alt_<group>` suffix so
//! cross-validation never silently drops them.

use std::collections::{BTreeSet, HashSet};

/// `(group-field id, canonical name)` table — exactly the registry from
/// spec §8 with the duplicates already collapsed.
const GF_TO_CANONICAL: &[(&str, &str)] = &[
    // 001 — engine basics
    ("001-1", "rpm"),
    ("001-2", "iq_actual"),
    ("001-3", "modulating_piston_v"),
    ("001-4", "coolant_c"),
    // 003 — MAF + EGR
    ("003-1", "rpm"),
    ("003-2", "maf_spec"),
    ("003-3", "maf_actual"),
    ("003-4", "egr_duty"),
    // 004 — sanity
    ("004-1", "rpm"),
    ("004-2", "battery_v"),
    ("004-3", "coolant_c"),
    ("004-4", "tdc_sensor"),
    // 008 — IQ + limiters
    ("008-1", "rpm"),
    ("008-2", "iq_requested"),
    ("008-3", "iq_limit_rpm"),
    ("008-4", "iq_limit_maf"),
    // 005 — engine speed / load / road speed / op status
    ("005-1", "rpm"),
    ("005-2", "load_pct"),
    ("005-3", "vehicle_speed"),
    // 010 — MAF + ambient + MAP-actual + accelerator pedal (G79)
    ("010-1", "maf_for_atm"),
    ("010-2", "atm_pressure"),
    ("010-3", "map_abs_010"),
    ("010-4", "pedal_pct"),
    // 011 — boost
    ("011-1", "rpm"),
    ("011-2", "boost_spec"),
    ("011-3", "boost_actual"),
    ("011-4", "n75_duty"),
    // 013 — smooth running
    ("013-1", "srcv_cyl1"),
    ("013-2", "srcv_cyl2"),
    ("013-3", "srcv_cyl3"),
    ("013-4", "fuel_temp_c"),
    // 015 — torque
    ("015-1", "rpm"),
    ("015-2", "torque_request"),
    ("015-3", "torque_actual"),
    // 020 — timing
    ("020-1", "rpm"),
    ("020-2", "soi_actual"),
    ("020-3", "map_abs"),
    ("020-4", "load_pct"),
    // 031 — variant of MAF
    ("031-2", "maf_spec"),
    ("031-3", "maf_actual"),
];

/// Look up the canonical channel name for a `NNN-K` id.
pub fn canonical_name(group_field_id: &str) -> Option<&'static str> {
    GF_TO_CANONICAL.iter().find_map(|&(k, v)| (k == group_field_id).then_some(v))
}

/// One column-map entry produced by [`build_column_map`].
#[derive(Debug, Clone)]
pub struct MappedColumn {
    /// Original `NNN-K` id from the VCDS header.
    pub gfid: String,
    /// Canonical name to use downstream (may have `__alt_<group>` suffix
    /// for duplicates).
    pub canonical: String,
}

/// Walk the column ids in order and produce `(mappings, unmapped_ids)`.
///
/// Duplicate canonical names get a `__alt_<group>` suffix so the parser
/// never drops them silently.
pub fn build_column_map(group_field_ids: &[String])
    -> (Vec<MappedColumn>, Vec<String>)
{
    let mut out = Vec::with_capacity(group_field_ids.len());
    let mut seen: HashSet<&'static str> = HashSet::new();
    let mut unmapped = Vec::new();
    for gfid in group_field_ids {
        match canonical_name(gfid) {
            Some(canonical) => {
                let canonical_str = if seen.contains(canonical) {
                    let group = gfid.split('-').next().unwrap_or("xxx");
                    format!("{canonical}__alt_{group}")
                } else {
                    seen.insert(canonical);
                    canonical.to_string()
                };
                out.push(MappedColumn { gfid: gfid.clone(), canonical: canonical_str });
            }
            None => unmapped.push(gfid.clone()),
        }
    }
    (out, unmapped)
}

/// Return the set of group ids (`"001"`, `"003"`, ...) present in a list
/// of `NNN-K` columns.
pub fn groups_present(group_field_ids: &[String]) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for gfid in group_field_ids {
        if let Some(g) = gfid.split('-').next() {
            out.insert(g.to_string());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_lookup_works() {
        assert_eq!(canonical_name("001-1"), Some("rpm"));
        assert_eq!(canonical_name("999-9"), None);
    }

    #[test]
    fn duplicates_get_alt_suffix() {
        let ids = vec!["001-1".to_string(), "003-1".to_string(), "011-1".to_string()];
        let (cols, _) = build_column_map(&ids);
        assert_eq!(cols[0].canonical, "rpm");
        assert_eq!(cols[1].canonical, "rpm__alt_003");
        assert_eq!(cols[2].canonical, "rpm__alt_011");
    }

    #[test]
    fn unmapped_returned_separately() {
        let ids = vec!["001-1".to_string(), "777-9".to_string()];
        let (cols, un) = build_column_map(&ids);
        assert_eq!(cols.len(), 1);
        assert_eq!(un, vec!["777-9".to_string()]);
    }

    #[test]
    fn groups_present_extracts_prefixes() {
        let ids = vec![
            "001-1".to_string(), "001-2".to_string(),
            "011-1".to_string(), "011-4".to_string(),
        ];
        let g = groups_present(&ids);
        assert_eq!(g.iter().cloned().collect::<Vec<_>>(), vec!["001", "011"]);
    }
}
