//! `ecu-shenanigans` CLI — analyse a VCDS log and write a Markdown report.

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};

use ecu_shenanigans::disclaimer::DISCLAIMER;
use ecu_shenanigans::ingest::parse_vcds_csv;
use ecu_shenanigans::recommend::{recommend, write_report};
use ecu_shenanigans::rules::analyse;
use ecu_shenanigans::util::resample_to_uniform;
use ecu_shenanigans::util::timebase::DEFAULT_RATE_HZ;
use ecu_shenanigans::VERSION;

#[derive(Debug, Parser)]
#[command(
    name = "ecu-shenanigans",
    version = VERSION,
    about = "Analyse VCDS .csv logs from a Skoda Fabia AMF / EDC15P+ \
             and emit sane Stage 1 tuning recommendations. \
             Read-only against the ECU; never writes the .bin.",
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Headless analysis — produce a report from a CSV.
    Analyse {
        /// VCDS `.csv` file path.
        path: PathBuf,
        /// Output directory for the report.
        #[arg(long, default_value = "out")]
        out: PathBuf,
    },
}

fn print_banner() {
    let line = "=".repeat(78);
    eprintln!("{line}");
    eprintln!("  ecu-shenanigans v{VERSION}");
    eprintln!("{line}");
    eprintln!("{DISCLAIMER}");
    eprintln!("{line}\n");
}

fn run_analyse(path: &std::path::Path, out_dir: &std::path::Path) -> Result<(), String> {
    if !path.exists() {
        return Err(format!("file not found: {}", path.display()));
    }
    eprintln!("parsing {} ...", path.display());
    let log = parse_vcds_csv(path).map_err(|e| e.to_string())?;
    for w in &log.warnings {
        eprintln!("  warning [{}]: {}", w.code, w.message);
    }
    let groups: Vec<&str> = log.groups.iter().map(String::as_str).collect();
    eprintln!("  groups: {groups:?}");
    eprintln!("  median dt: {:.0} ms", log.median_sample_dt_ms);
    eprintln!("  rows: {}", log.len());

    let df = resample_to_uniform(&log, DEFAULT_RATE_HZ);
    let result = analyse(df, log);
    eprintln!(
        "  pulls: {} · findings: {} (crit={} warn={} info={})",
        result.pulls.len(), result.findings.len(),
        result.critical().len(), result.warn().len(), result.info().len(),
    );

    let recs = recommend(&result.findings);
    let md_path = write_report(&result, &recs, out_dir).map_err(|e| e.to_string())?;
    eprintln!("  report: {}", md_path.display());
    Ok(())
}

fn main() -> ExitCode {
    print_banner();
    let cli = Cli::parse();
    let Some(command) = cli.command else {
        eprintln!("no command given. Run `ecu-shenanigans --help` for usage.");
        return ExitCode::from(2);
    };
    match command {
        Command::Analyse { path, out } => match run_analyse(&path, &out) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("error: {e}");
                ExitCode::from(1)
            }
        },
    }
}
