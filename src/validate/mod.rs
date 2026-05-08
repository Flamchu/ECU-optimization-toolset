//! Post-flash validation routines.
//!
//! The v3 spec adds a dedicated EGR-delete validation step that runs
//! against a post-flash log to verify the delete was applied correctly.
//! See [`egr_delete::validate_egr_delete`] for the 15-item checklist.

pub mod egr_delete;

pub use egr_delete::{validate_egr_delete, CheckOutcome, CheckStatus, ValidationReport};
