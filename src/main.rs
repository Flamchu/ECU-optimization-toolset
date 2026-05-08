//! `ecu-shenanigans` CLI — analyse VCDS logs, validate EGR delete,
//! emit a Markdown report.

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};

use ecu_shenanigans::disclaimer::DISCLAIMER;
use ecu_shenanigans::ingest::{parse_vcds_csv, parse_vcds_csv_with_dtc};
use ecu_shenanigans::recommend::{recommend, render_markdown};
use ecu_shenanigans::rules::analyse;
use ecu_shenanigans::util::resample_to_uniform;
use ecu_shenanigans::util::timebase::DEFAULT_RATE_HZ;
use ecu_shenanigans::validate::{validate_egr_delete, validate_egr_delete_pre_post};
use ecu_shenanigans::VERSION;

#[derive(Debug, Parser)]
#[command(
    name = "ecu-shenanigans",
    version = VERSION,
    about = "Analyse VCDS .csv logs from a Skoda Fabia AMF / EDC15P+ \
             (Garrett GT1544S) and emit sane Stage 1 tuning recommendations. \
             Read-only against the ECU; never writes the .bin.",
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Read VCDS CSV(s), run rules, emit Markdown.
    Analyse {
        /// VCDS `.csv` file path (repeat for triple-group capture bundles).
        #[arg(long, short, required = true, num_args = 1..)]
        input: Vec<PathBuf>,

        /// Optional DTC sidecar text file (one DTC per line).
        #[arg(long)]
        dtc: Option<PathBuf>,

        /// Append the §10 EGR-delete validation checklist to the report.
        #[arg(long)]
        validate: bool,

        /// Output Markdown path. Defaults to stdout.
        #[arg(long)]
        out: Option<PathBuf>,

        /// Acknowledge the §0 disclaimer. **Mandatory** — the tool refuses
        /// to run otherwise (exit 2).
        #[arg(long)]
        accept_disclaimer: bool,
    },
    /// Run the §10 15-item post-EGR-delete validation checklist.
    /// Exits 0 on PASS, 2 on FAIL.
    ValidateEgrDelete {
        /// Pre-delete VCDS log (for items 11/12 cross-checks).
        #[arg(long)]
        pre: Option<PathBuf>,

        /// Post-delete VCDS log (the post-flash log to validate).
        #[arg(long)]
        post: PathBuf,

        /// Optional DTC sidecar text file for the post-delete log.
        #[arg(long)]
        dtc: Option<PathBuf>,

        /// Output Markdown path. Defaults to stdout.
        #[arg(long)]
        out: Option<PathBuf>,

        /// Acknowledge the §0 disclaimer.
        #[arg(long)]
        accept_disclaimer: bool,
    },
}

fn print_banner() {
    let line = "=".repeat(78);
    eprintln!("{line}");
    eprintln!("  ecu-shenanigans v{VERSION}  (AMF · EDC15P+ · Garrett GT1544S)");
    eprintln!("{line}");
    eprintln!("{DISCLAIMER}");
    eprintln!("{line}\n");
}

fn require_disclaimer(accepted: bool) -> Result<(), ExitCode> {
    if accepted {
        return Ok(());
    }
    eprintln!(
        "error: --accept-disclaimer is required. Re-read the §0 disclaimer above and pass \
         the flag explicitly to acknowledge."
    );
    Err(ExitCode::from(2))
}

fn run_analyse(
    inputs: &[PathBuf],
    dtc: Option<&PathBuf>,
    validate: bool,
    out: Option<&PathBuf>,
) -> Result<(), String> {
    if inputs.is_empty() {
        return Err("no --input files supplied".to_string());
    }
    // For now we operate on the first input log; multi-input merge is
    // an open extension hook (not required by acceptance criteria).
    let primary = &inputs[0];
    if !primary.exists() {
        return Err(format!("file not found: {}", primary.display()));
    }
    if inputs.len() > 1 {
        eprintln!(
            "note: {} input(s) supplied; analysing the first ({}) — multi-bundle merge is \
             a future extension. Other inputs were ignored.",
            inputs.len(),
            primary.display(),
        );
    }
    eprintln!("parsing {} ...", primary.display());
    let log = match dtc {
        Some(dtc_path) => parse_vcds_csv_with_dtc(primary, dtc_path).map_err(|e| e.to_string())?,
        None => parse_vcds_csv(primary).map_err(|e| e.to_string())?,
    };
    for w in &log.warnings {
        eprintln!("  warning [{}]: {}", w.code, w.message);
    }
    let groups: Vec<&str> = log.groups.iter().map(String::as_str).collect();
    eprintln!("  groups: {groups:?}");
    eprintln!("  median dt: {:.0} ms", log.median_sample_dt_ms);
    eprintln!("  rows: {}", log.len());
    if !log.dtcs.is_empty() {
        eprintln!("  dtcs: {:?}", log.dtcs);
    }

    let df = resample_to_uniform(&log, DEFAULT_RATE_HZ);
    let validation = if validate { Some(validate_egr_delete(&log)) } else { None };
    let result = analyse(df, log);
    eprintln!(
        "  pulls: {} · findings: {} (crit={} warn={} info={})",
        result.pulls.len(), result.findings.len(),
        result.critical().len(), result.warn().len(), result.info().len(),
    );

    let recs = recommend(&result.findings);
    let md = render_markdown(&result, &recs, validation.as_ref());

    match out {
        Some(path) => {
            std::fs::write(path, md).map_err(|e| e.to_string())?;
            eprintln!("  report: {}", path.display());
        }
        None => println!("{md}"),
    }
    Ok(())
}

fn run_validate_egr_delete(
    pre: Option<&PathBuf>,
    post: &PathBuf,
    dtc: Option<&PathBuf>,
    out: Option<&PathBuf>,
) -> Result<bool, String> {
    if !post.exists() {
        return Err(format!("file not found: {}", post.display()));
    }
    eprintln!("validating EGR delete on {} ...", post.display());
    let post_log = match dtc {
        Some(dtc_path) => parse_vcds_csv_with_dtc(post, dtc_path).map_err(|e| e.to_string())?,
        None => parse_vcds_csv(post).map_err(|e| e.to_string())?,
    };

    let report = if let Some(pre_path) = pre {
        if !pre_path.exists() {
            return Err(format!("pre-delete file not found: {}", pre_path.display()));
        }
        let pre_log = parse_vcds_csv(pre_path).map_err(|e| e.to_string())?;
        validate_egr_delete_pre_post(&pre_log, &post_log)
    } else {
        validate_egr_delete(&post_log)
    };

    let md = report.to_markdown();
    match out {
        Some(path) => {
            std::fs::write(path, &md).map_err(|e| e.to_string())?;
            eprintln!("  report: {}", path.display());
        }
        None => println!("{md}"),
    }
    Ok(report.pass())
}

fn main() -> ExitCode {
    print_banner();
    let cli = Cli::parse();
    let Some(command) = cli.command else {
        eprintln!("no command given. Run `ecu-shenanigans --help` for usage.");
        return ExitCode::from(2);
    };
    match command {
        Command::Analyse { input, dtc, validate, out, accept_disclaimer } => {
            if let Err(code) = require_disclaimer(accept_disclaimer) { return code; }
            match run_analyse(&input, dtc.as_ref(), validate, out.as_ref()) {
                Ok(()) => ExitCode::SUCCESS,
                Err(e) => {
                    eprintln!("error: {e}");
                    ExitCode::from(1)
                }
            }
        }
        Command::ValidateEgrDelete { pre, post, dtc, out, accept_disclaimer } => {
            if let Err(code) = require_disclaimer(accept_disclaimer) { return code; }
            match run_validate_egr_delete(pre.as_ref(), &post, dtc.as_ref(), out.as_ref()) {
                Ok(true) => ExitCode::SUCCESS,
                Ok(false) => ExitCode::from(2),
                Err(e) => {
                    eprintln!("error: {e}");
                    ExitCode::from(1)
                }
            }
        }
    }
}
