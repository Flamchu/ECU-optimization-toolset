//! AMF (1.4 TDI PD R3) on Bosch EDC15P+ — the only supported platform.

pub mod channels;
pub mod default_deltas;
pub mod egr;
pub mod envelope;
pub mod maps;
pub mod stock_refs;

/// Stable platform identifier used in reports and logs.
pub const PLATFORM_ID: &str = "amf_edc15p";

/// Human-readable platform label.
pub const PLATFORM_DISPLAY: &str = "Skoda Fabia 1.4 TDI PD · AMF · EDC15P+";
