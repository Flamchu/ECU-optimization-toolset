//! AMF rule pack and the runner that applies it across detected pulls.

pub mod base;
pub mod pack;
pub mod runner;

pub use base::{make_skipped, Finding, Rule, Severity};
pub use pack::ALL_RULES;
pub use runner::{analyse, run_rules, AnalysisResult};
