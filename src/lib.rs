//! `ecu-shenanigans` — single-platform VCDS datalog analyzer for the
//! Škoda Fabia Mk1 1.4 TDI PD (engine code AMF, Bosch EDC15P+).
//!
//! The library is read-only against the ECU; it ingests VCDS `.csv`
//! exports, runs a fixed AMF rule pack, and emits Stage 1 recommendations
//! clamped to a hard longevity envelope. Suggestions are symbolic and
//! intended to be pasted into a third-party tuning tool by hand.
//!
//! See [`disclaimer::DISCLAIMER`] for the verbatim safety notice.

#![deny(rust_2018_idioms)]
#![warn(missing_docs)]

pub mod disclaimer;
pub mod error;
pub mod ingest;
pub mod platform;
pub mod recommend;
pub mod rules;
pub mod util;

/// Library version, kept in sync with `Cargo.toml`.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
