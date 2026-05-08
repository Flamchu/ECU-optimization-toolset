//! VCDS log ingestion: CSV parser plus the canonicalizer that maps
//! `NNN-K` group-field ids to channel names.

pub mod canonicalize;
pub mod vcds;

pub use canonicalize::{build_column_map, canonical_name, groups_present};
pub use vcds::{parse_vcds_csv, ParseWarning, VcdsLog};
