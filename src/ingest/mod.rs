//! VCDS log ingestion: CSV parser, the canonicalizer that maps `NNN-K`
//! group-field ids to channel names, and the sidecar DTC file reader.

pub mod canonicalize;
pub mod dtc;
pub mod vcds;

pub use canonicalize::{build_column_map, canonical_name, groups_present};
pub use dtc::{parse_dtc_text, read_sidecar, sidecar_path_for};
pub use vcds::{parse_vcds_csv, parse_vcds_csv_with_dtc, ParseWarning, VcdsLog};
