//! Recommendation engine + Markdown report writer.

pub mod engine;
pub mod report;

pub use engine::{recommend, Recommendation, Status};
pub use report::{render_markdown, write_report};
