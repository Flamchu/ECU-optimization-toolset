//! Markdown report writer.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use chrono::Utc;

use crate::disclaimer::DISCLAIMER;
use crate::platform::amf_edc15p::egr::recommend_egr_delete_deltas;
use crate::platform::amf_edc15p::envelope::CAPS;
use crate::platform::amf_edc15p::PLATFORM_DISPLAY;
use crate::recommend::engine::Recommendation;
use crate::rules::base::{Finding, Severity};
use crate::rules::runner::AnalysisResult;
use crate::validate::ValidationReport;

fn now_iso() -> String {
    Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

fn now_compact_utc() -> String {
    Utc::now().format("%Y%m%dT%H%M%SZ").to_string()
}

fn fmt_num(x: f64) -> String {
    if x.fract() == 0.0 { format!("{x:.0}") } else { format!("{x:.4}") }
}

fn severity_rank(s: Severity) -> u8 {
    match s { Severity::Critical => 2, Severity::Warn => 1, Severity::Info => 0 }
}

/// Render the full Markdown report to a string. An optional
/// [`ValidationReport`] is appended as a Markdown subsection when
/// supplied (CLI `--validate`).
pub fn render_markdown(
    result: &AnalysisResult,
    recommendations: &[Recommendation],
    validation: Option<&ValidationReport>,
) -> String {
    let mut lines: Vec<String> = Vec::new();
    lines.push("# ecu-shenanigans — Analysis report".to_string());
    lines.push(String::new());
    lines.push(format!("> {DISCLAIMER}"));
    lines.push(String::new());
    lines.push(format!("- Platform: `{PLATFORM_DISPLAY}`"));
    let src = result.log.source_file.file_name()
        .and_then(|s| s.to_str()).unwrap_or("(unknown)");
    lines.push(format!("- Source: `{src}`"));
    lines.push(format!("- Generated: `{}`", now_iso()));
    let groups: Vec<&str> = result.log.groups.iter().map(String::as_str).collect();
    let groups_str = if groups.is_empty() { "(none)".to_string() } else { groups.join(", ") };
    lines.push(format!("- Groups present: `{groups_str}`"));
    let low = if result.log.low_rate() { " — **LOW_RATE**" } else { "" };
    lines.push(format!(
        "- Median sample interval: `{:.0} ms`{}",
        result.log.median_sample_dt_ms, low,
    ));
    lines.push(format!("- Pulls detected: `{}`", result.pulls.len()));
    lines.push(format!(
        "- DTCs from sidecar: `{}`",
        if result.log.dtcs.is_empty() { "(none / no sidecar)".to_string() } else { result.log.dtcs.join(", ") },
    ));
    lines.push(String::new());

    if !result.log.warnings.is_empty() {
        lines.push("## Parser warnings".to_string());
        for w in &result.log.warnings {
            lines.push(format!("- **{}**: {}", w.code, w.message));
        }
        lines.push(String::new());
    }

    // ---- EGR Delete Strategy section ----------------------------------
    lines.push("## EGR Delete Strategy".to_string());
    lines.push(String::new());
    lines.push(
        "Software-only EGR delete is mandatory. Hardware (EGR valve, cooler, \
         vacuum lines, ASV) stays installed; the vacuum-actuated valve is \
         held closed by its return spring with 0 % duty in both banks of \
         the AGR map. The MAF/MAP smoke switch is **explicitly unchanged** — \
         MAF stays the closed-loop smoke-limiter input."
            .to_string(),
    );
    lines.push(String::new());
    lines.push("| Map | Cells | Action | Rationale |".to_string());
    lines.push("|---|---|---|---|".to_string());
    for d in recommend_egr_delete_deltas() {
        let rationale = d.rationale.replace('\n', " ");
        lines.push(format!(
            "| `{}` | {} | {} | {} |",
            d.map_name, d.cell_selector, d.action, rationale,
        ));
    }
    lines.push(String::new());
    lines.push(format!(
        "Hard envelope: λ ≥ {}, peak IQ ≤ {} mg/stroke, EGR duty = {} %, \
         spec-MAF ≥ {} mg/stroke, peak boost ≤ {} mbar (≤ {} above 4000 rpm), \
         modelled torque ≤ {} Nm, SOI ≤ {}° BTDC at IQ ≥ {} mg, EOI ≤ {}° ATDC, \
         coolant pull-min {} °C, warm-cruise/idle min {} °C.",
        CAPS.lambda_floor, CAPS.peak_iq_mg, CAPS.egr_duty_max_pct,
        CAPS.spec_maf_fill_mg_stroke, CAPS.peak_boost_mbar_abs,
        CAPS.peak_boost_above_4000rpm_mbar_abs,
        CAPS.modelled_flywheel_torque_nm, CAPS.soi_max_btdc, CAPS.soi_iq_threshold_mg,
        CAPS.eoi_max_atdc, CAPS.coolant_pull_min_c, CAPS.warm_coolant_min_c,
    ));
    lines.push(String::new());

    if result.pulls.is_empty() {
        lines.push("## No WOT pulls detected".to_string());
        lines.push(format!(
            "A pull requires pedal ≥ {} % AND RPM rising AND duration ≥ 2 s. \
             Re-log with full WOT acceleration runs from at least 2000 to 4500 rpm. \
             Global rules (R16, R19, R21) may still produce findings.",
             CAPS.pedal_wot_pct
        ));
        lines.push(String::new());
    }

    // Findings always rendered (global rules can fire without pulls).
    if !result.findings.is_empty() {
        lines.push("## Findings".to_string());
        lines.push(String::new());
        lines.push("| Pull | Rule | Severity | Observed | Threshold | Why |".to_string());
        lines.push("|---|---|---|---|---|---|".to_string());
        let mut sorted: Vec<&Finding> = result.findings.iter().collect();
        sorted.sort_by(|a, b| {
            severity_rank(b.severity).cmp(&severity_rank(a.severity))
                .then_with(|| a.rule_id.cmp(b.rule_id))
                .then_with(|| a.pull_id.cmp(&b.pull_id))
        });
        for f in &sorted {
            let (obs, thr) = if f.skipped {
                ("—".to_string(), "—".to_string())
            } else {
                (fmt_num(f.observed_extreme), fmt_num(f.threshold))
            };
            let rationale = f.rationale.replace('\n', " ");
            let pull_label = if f.pull_id == 0 { "G".to_string() } else { f.pull_id.to_string() };
            lines.push(format!(
                "| {} | `{}` | {} | {} | {} | {} |",
                pull_label, f.rule_id, f.severity.as_str(), obs, thr, rationale,
            ));
        }
        lines.push(String::new());
        lines.push(
            "_Pull `G` denotes a finding from a global-scope rule (R16, R19, R21) \
             evaluated once over the entire log rather than per-pull._".to_string()
        );
        lines.push(String::new());
    }

    if !result.pulls.is_empty() {
        lines.push("## Per-pull summary".to_string());
        lines.push(String::new());
        for pull in &result.pulls {
            lines.push(format!(
                "### Pull {} — t={:.1}s..{:.1}s, RPM {:.0}→{:.0}, dur {:.1}s",
                pull.pull_id, pull.t_start, pull.t_end,
                pull.rpm_start, pull.rpm_end, pull.duration_s(),
            ));
            let pull_findings: Vec<&Finding> = result.findings.iter()
                .filter(|f| f.pull_id == pull.pull_id && !f.skipped)
                .collect();
            if pull_findings.is_empty() {
                lines.push("- No findings; pull is within envelope.".to_string());
            } else {
                for f in pull_findings {
                    lines.push(format!(
                        "- **[{}] {}** — {} (observed {}, threshold {})",
                        f.severity.as_str().to_ascii_uppercase(),
                        f.rule_id, f.rationale.replace('\n', " "),
                        fmt_num(f.observed_extreme), fmt_num(f.threshold),
                    ));
                }
            }
            lines.push(String::new());
        }
    }

    lines.push("## Recommendation table".to_string());
    lines.push(String::new());
    lines.push("| Map | Cell selector | Status | Proposed | Rule refs | Rationale |".to_string());
    lines.push("|---|---|---|---|---|---|".to_string());
    for r in recommendations {
        let rules = if r.rule_refs.is_empty() { "—".to_string() } else { r.rule_refs.join(", ") };
        let rationale = r.rationale.replace('\n', "<br>");
        lines.push(format!(
            "| `{}` | {} | **{}** | {} | {} | {} |",
            r.map_name, r.cell_selector, r.status.as_str(),
            r.proposed_value_text, rules, rationale,
        ));
    }
    lines.push(String::new());

    if !result.skipped_rules.is_empty() {
        lines.push("## Rules SKIPPED (group missing)".to_string());
        for rid in &result.skipped_rules {
            lines.push(format!("- `{rid}`"));
        }
        lines.push(String::new());
    }

    if let Some(report) = validation {
        lines.push(report.to_markdown());
        lines.push(String::new());
    }

    lines.join("\n")
}

/// Write `report_<utc-timestamp>.md` to `out_dir`. Returns the path.
pub fn write_report(
    result: &AnalysisResult,
    recommendations: &[Recommendation],
    validation: Option<&ValidationReport>,
    out_dir: &Path,
) -> io::Result<PathBuf> {
    fs::create_dir_all(out_dir)?;
    let md = render_markdown(result, recommendations, validation);
    let path = out_dir.join(format!("report_{}.md", now_compact_utc()));
    fs::write(&path, md)?;
    Ok(path)
}
