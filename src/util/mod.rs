//! Small numeric utilities: 5 Hz resampler and WOT-pull detection.

pub mod pulls;
pub mod timebase;

pub use pulls::{detect_pulls, Pull};
pub use timebase::{resample_to_uniform, ResampledLog, DEFAULT_RATE_HZ};
