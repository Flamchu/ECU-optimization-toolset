//! Error types used across the library.

use std::path::PathBuf;

use thiserror::Error;

/// Top-level error type for the analyser pipeline.
#[derive(Debug, Error)]
pub enum Error {
    /// File could not be opened or read.
    #[error("could not read {path}: {source}")]
    Io {
        /// Source path that failed.
        path: PathBuf,
        /// Underlying I/O error.
        #[source]
        source: std::io::Error,
    },

    /// VCDS log header could not be located.
    #[error("could not locate VCDS data header row (no row contains ≥3 'NNN-K' tokens). Is this a VCDS .csv export?")]
    NotVcds,

    /// Required VCDS group(s) missing for the requested operation.
    #[error("missing required VCDS groups: {0:?}")]
    MissingGroups(Vec<String>),
}

/// Result alias used throughout the crate.
pub type Result<T> = std::result::Result<T, Error>;
