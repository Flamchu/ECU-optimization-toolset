//! Post-flash validation routines.
//!
//! The v4 spec mandates a dedicated EGR-delete validation step that runs
//! against a post-flash log (and optionally a pre-delete log) to verify
//! the delete was applied correctly. See
//! [`egr_delete::validate_egr_delete`] (single-log) and
//! [`egr_delete::validate_egr_delete_pre_post`] (paired) for the 15-item
//! checklist.

pub mod egr_delete;

pub use egr_delete::{
    validate_egr_delete, validate_egr_delete_pre_post,
    CheckOutcome, CheckStatus, ValidationReport,
};
