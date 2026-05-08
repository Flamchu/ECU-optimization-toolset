//! Project-wide disclaimer text — verbatim per spec §11.
//!
//! Must appear in the CLI banner and the exported report header. Edit this
//! string and every consumer updates with it.

/// Verbatim safety / legal notice. Reproduced unchanged from the
/// specification (spec §11).
pub const DISCLAIMER: &str = "**`ecu-shenanigans` is an analysis and educational tool. It does NOT modify your ECU. Any tuning changes are performed at the user's sole risk, on private property only, on a vehicle the user owns. Modifying engine calibration may void your warranty, render the vehicle non-roadworthy, contravene type-approval / emissions regulations in your jurisdiction (e.g. EU Regulation 2018/858, UK MOT diesel smoke limits, US CAA §203), and may damage the engine, turbocharger, clutch, or particulate after-treatment. The \"sane Stage 1\" envelope encoded in this tool is a conservative community heuristic, not a manufacturer specification. The authors accept no liability. If the tool says BLOCKED — envelope cap, do not work around it.**";
