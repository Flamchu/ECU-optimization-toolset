//! VCDS CSV parser.
//!
//! VCDS exports a multi-row banner before the data, plus per-locale
//! decimal quirks. The expected shape (spec §6.2):
//!
//! ```text
//! Row 1: Group A:,001,Group B:,003,Group C:,011, ...
//! Row 2: <field-name-001-1>,<field-name-001-2>, ...
//! Row 3: <unit-001-1>,<unit-001-2>, ...
//! Row 4: TIME,STAMP,001-1,001-2, ... ,011-4
//! Row 5+: data rows
//! ```
//!
//! Rows 1 and 3 may be missing on some VCDS releases; the data column
//! header is the only one we anchor on.
//!
//! Locale handling:
//!
//! - Decimal-comma + semicolon-separator (EU)
//! - Decimal-dot + comma-separator (US)
//!
//! Detected by sniffing the first data row.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};

use crate::error::{Error, Result};
use crate::ingest::canonicalize::{build_column_map, groups_present};
use crate::ingest::dtc;
use crate::platform::amf_edc15p::channels::MIN_REQUIRED_GROUPS;

/// One soft warning surfaced from the parser.
#[derive(Debug, Clone)]
pub struct ParseWarning {
    /// Short machine-readable code (e.g. `"LOW_RATE"`).
    pub code: String,
    /// Human-readable explanation.
    pub message: String,
}

/// Parsed VCDS log: time-indexed dataframe-like structure plus metadata.
#[derive(Debug, Clone)]
pub struct VcdsLog {
    /// Source path the log was parsed from.
    pub source_file: PathBuf,
    /// Time axis in seconds, normalised so the first sample is 0.
    pub time: Vec<f64>,
    /// Channel name → sample values (length matches `time`). NaN means
    /// the value was missing on that row.
    pub data: BTreeMap<String, Vec<f64>>,
    /// Set of VCDS groups present in the log (`"001"`, `"003"`, ...).
    pub groups: BTreeSet<String>,
    /// Field-name row keyed by `NNN-K` id, where present.
    pub field_names: HashMap<String, String>,
    /// Unit row keyed by `NNN-K` id, where present.
    pub units: HashMap<String, String>,
    /// `NNN-K` columns we did not recognise.
    pub unmapped_columns: Vec<String>,
    /// Soft warnings collected during parsing.
    pub warnings: Vec<ParseWarning>,
    /// Median sample interval in milliseconds.
    pub median_sample_dt_ms: f64,
    /// DTCs from a sidecar `.dtc.txt` scan file, if loaded. Empty means
    /// either no sidecar was present, or no DTCs were captured. R19 and
    /// the EGR-delete checklist read this list.
    pub dtcs: Vec<String>,
}

impl VcdsLog {
    /// Whether the median sample interval exceeded 350 ms (spec §6.3).
    pub fn low_rate(&self) -> bool {
        self.median_sample_dt_ms > 350.0
    }

    /// Whether the log contains the minimum required VCDS groups for
    /// pull analysis.
    pub fn has_required_groups(&self) -> bool {
        MIN_REQUIRED_GROUPS.iter().all(|g| self.groups.contains(*g))
    }

    /// Required groups missing from the log.
    pub fn missing_required_groups(&self) -> Vec<String> {
        MIN_REQUIRED_GROUPS.iter()
            .filter(|g| !self.groups.contains(**g))
            .map(|s| s.to_string())
            .collect()
    }

    /// Number of sample rows.
    pub fn len(&self) -> usize {
        self.time.len()
    }

    /// Whether the log has zero rows.
    pub fn is_empty(&self) -> bool {
        self.time.is_empty()
    }
}

// ---------------------------------------------------------------------------
// Locale + delimiter sniffing
// ---------------------------------------------------------------------------

/// Pick the most-common separator from the sample.
fn sniff_delimiter(sample: &str) -> char {
    let candidates = [',', ';', '\t'];
    let mut best = ',';
    let mut best_count = 0;
    for &c in &candidates {
        let count = sample.chars().filter(|ch| *ch == c).count();
        if count > best_count {
            best_count = count;
            best = c;
        }
    }
    best
}

/// Treat as EU locale only if the separator is `;` and the sample has
/// commas but no decimal points.
fn is_eu_locale(sample_data_row: &str, delimiter: char) -> bool {
    if delimiter != ';' {
        return false;
    }
    let first_field = sample_data_row.split(',').next().unwrap_or("");
    sample_data_row.contains(',') && !first_field.contains('.')
}

/// Parse a single value. Empty / unparseable becomes `NaN`.
fn to_float(s: &str, eu: bool) -> f64 {
    let trimmed = s.trim().trim_matches('"');
    if trimmed.is_empty() {
        return f64::NAN;
    }
    let cleaned = if eu {
        trimmed.replace('.', "").replace(',', ".")
    } else {
        trimmed.to_string()
    };
    cleaned.parse::<f64>().unwrap_or(f64::NAN)
}

// ---------------------------------------------------------------------------
// Header anchoring
// ---------------------------------------------------------------------------

/// Match a VCDS data column id like `"001-1"` or `"011-4"`.
fn is_nnn_k(token: &str) -> bool {
    let t = token.trim();
    let mut parts = t.splitn(2, '-');
    let head = match parts.next() { Some(s) => s, None => return false };
    let tail = match parts.next() { Some(s) => s, None => return false };
    if head.len() != 3 || !head.chars().all(|c| c.is_ascii_digit()) {
        return false;
    }
    !tail.is_empty() && tail.chars().all(|c| c.is_ascii_digit())
}

/// Locate the row whose tokens (after the optional TIME/STAMP) match
/// `NNN-K`. Returns its index in the row list.
fn find_data_header_row(rows: &[Vec<String>]) -> Result<usize> {
    for (idx, row) in rows.iter().enumerate() {
        let count = row.iter().filter(|c| is_nnn_k(c)).count();
        if count >= 3 {
            return Ok(idx);
        }
    }
    Err(Error::NotVcds)
}

/// Split `text` into trimmed-cell rows on `delimiter`, dropping fully
/// empty lines.
fn split_rows(text: &str, delimiter: char) -> Vec<Vec<String>> {
    text.lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.split(delimiter).map(|c| c.trim().to_string()).collect())
        .collect()
}

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

/// Parse a VCDS `.csv` export at `path`. Auto-loads `<base>.dtc.txt`
/// alongside if present (silent if not).
pub fn parse_vcds_csv<P: AsRef<Path>>(path: P) -> Result<VcdsLog> {
    let p = path.as_ref().to_path_buf();
    let raw = std::fs::read_to_string(&p)
        .map_err(|e| Error::Io { path: p.clone(), source: e })?;
    let mut log = parse_vcds_str(&p, &raw)?;
    // Auto-load conventional sidecar.
    let sidecar = dtc::sidecar_path_for(&p);
    if let Ok(codes) = dtc::read_sidecar(&sidecar) {
        log.dtcs = codes;
    }
    Ok(log)
}

/// Parse a VCDS `.csv` and an explicit DTC sidecar path (overrides the
/// conventional `<base>.dtc.txt` lookup). Use this when the operator
/// passes `--dtc <FILE>` on the CLI.
pub fn parse_vcds_csv_with_dtc<P: AsRef<Path>, D: AsRef<Path>>(
    csv: P,
    dtc_path: D,
) -> Result<VcdsLog> {
    let p = csv.as_ref().to_path_buf();
    let raw = std::fs::read_to_string(&p)
        .map_err(|e| Error::Io { path: p.clone(), source: e })?;
    let mut log = parse_vcds_str(&p, &raw)?;
    log.dtcs = dtc::read_sidecar(&dtc_path).unwrap_or_default();
    Ok(log)
}

/// Variant of [`parse_vcds_csv`] for in-memory text.
pub fn parse_vcds_str(path: &Path, raw: &str) -> Result<VcdsLog> {
    // Strip UTF-8 BOM if present.
    let raw = raw.strip_prefix('\u{feff}').unwrap_or(raw);

    // Sniff delimiter from the first 8 non-empty lines.
    let sample_lines: Vec<&str> = raw.lines().filter(|l| !l.trim().is_empty()).take(8).collect();
    let sample_text = sample_lines.join("\n");
    let delimiter = sniff_delimiter(&sample_text);

    let rows = split_rows(raw, delimiter);
    let header_idx = find_data_header_row(&rows)?;
    let header_row = &rows[header_idx];
    let data_rows = &rows[header_idx + 1..];

    // Detect EU locale by inspecting the first data row.
    let eu = data_rows.first()
        .map(|r| is_eu_locale(&r.join(&delimiter.to_string()), delimiter))
        .unwrap_or(false);

    // Locate TIME/STAMP and NNN-K columns.
    let mut time_col_idx: Option<usize> = None;
    let mut nnn_indices: Vec<usize> = Vec::new();
    for (i, h) in header_row.iter().enumerate() {
        let upper = h.to_ascii_uppercase();
        if upper == "TIME" {
            time_col_idx = Some(i);
        } else if is_nnn_k(h) {
            nnn_indices.push(i);
        }
    }

    let nnn_ids: Vec<String> = nnn_indices.iter().map(|&i| header_row[i].clone()).collect();
    let (mapped, unmapped) = build_column_map(&nnn_ids);
    let groups = groups_present(&nnn_ids);

    // Pull field-names and units from the rows preceding the data header,
    // if they look right (token count is at least header_len - 1).
    let mut field_names: HashMap<String, String> = HashMap::new();
    let mut units: HashMap<String, String> = HashMap::new();
    for offset in [1usize, 2] {
        if header_idx >= offset {
            let candidate = &rows[header_idx - offset];
            if candidate.len() + 1 >= header_row.len() {
                let target = if offset == 2 { &mut field_names } else { &mut units };
                for (j, gfid) in nnn_indices.iter().zip(nnn_ids.iter()) {
                    if *j < candidate.len() {
                        target.insert(gfid.clone(), candidate[*j].clone());
                    }
                }
            }
        }
    }

    // Build the time-indexed columns.
    let mut columns: BTreeMap<String, Vec<f64>> = BTreeMap::new();
    for col in &mapped {
        columns.insert(col.canonical.clone(), Vec::with_capacity(data_rows.len()));
    }
    let mut times: Vec<f64> = Vec::with_capacity(data_rows.len());

    for r in data_rows {
        let Some(ti) = time_col_idx else { continue };
        if r.len() <= ti {
            continue;
        }
        let t = to_float(&r[ti], eu);
        if t.is_nan() {
            continue;
        }
        times.push(t);
        for (j, gfid) in nnn_indices.iter().zip(nnn_ids.iter()) {
            let col = mapped.iter().find(|m| &m.gfid == gfid);
            if let Some(col) = col {
                let v = if *j < r.len() { to_float(&r[*j], eu) } else { f64::NAN };
                columns.get_mut(&col.canonical).expect("column registered above").push(v);
            }
        }
    }

    // Pad short columns with NaN to match `times`.
    let n = times.len();
    for vals in columns.values_mut() {
        if vals.len() < n {
            vals.extend(std::iter::repeat(f64::NAN).take(n - vals.len()));
        } else if vals.len() > n {
            vals.truncate(n);
        }
    }

    // Normalise the time axis so the first sample is at t=0.
    if let Some(&t0) = times.first() {
        for t in times.iter_mut() { *t -= t0; }
    }
    sort_by_time(&mut times, &mut columns);

    // Median sample interval.
    let median_dt_ms = if times.len() > 1 {
        let mut diffs: Vec<f64> = times.windows(2)
            .map(|w| w[1] - w[0])
            .filter(|d| d.is_finite() && *d > 0.0)
            .collect();
        if diffs.is_empty() { 0.0 } else {
            diffs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            diffs[diffs.len() / 2] * 1000.0
        }
    } else { 0.0 };

    let mut warnings = Vec::new();
    let mut log = VcdsLog {
        source_file: path.to_path_buf(),
        time: times,
        data: columns,
        groups,
        field_names,
        units,
        unmapped_columns: unmapped,
        warnings: Vec::new(),
        median_sample_dt_ms: median_dt_ms,
        dtcs: Vec::new(),
    };
    if log.low_rate() {
        warnings.push(ParseWarning {
            code: "LOW_RATE".to_string(),
            message: format!(
                "Median sample interval {median_dt_ms:.0} ms > 350 ms — \
                 R09/R10 will be downgraded to warn."
            ),
        });
    }
    if !log.has_required_groups() {
        warnings.push(ParseWarning {
            code: "MISSING_GROUPS".to_string(),
            message: format!(
                "Groups {:?} required for pull analysis; some rules will SKIP.",
                log.missing_required_groups()
            ),
        });
    }
    log.warnings = warnings;
    Ok(log)
}

/// Sort a `time -> values...` table in place by the time axis.
fn sort_by_time(times: &mut Vec<f64>, columns: &mut BTreeMap<String, Vec<f64>>) {
    let n = times.len();
    if n < 2 {
        return;
    }
    let mut indices: Vec<usize> = (0..n).collect();
    indices.sort_by(|&a, &b| times[a].partial_cmp(&times[b]).unwrap_or(std::cmp::Ordering::Equal));
    let needs_sort = indices.iter().enumerate().any(|(i, &j)| i != j);
    if !needs_sort {
        return;
    }
    let new_times: Vec<f64> = indices.iter().map(|&i| times[i]).collect();
    *times = new_times;
    for vals in columns.values_mut() {
        let new_vals: Vec<f64> = indices.iter().map(|&i| vals[i]).collect();
        *vals = new_vals;
    }
}

/// Cheap probe: do the first 8 lines contain `NNN-K` data column ids?
pub fn detect_is_vcds<P: AsRef<Path>>(path: P) -> bool {
    let Ok(text) = std::fs::read_to_string(path.as_ref()) else { return false; };
    let head: String = text.lines().take(8).collect::<Vec<_>>().join("\n");
    head.split(|c: char| !c.is_ascii_alphanumeric() && c != '-')
        .any(is_nnn_k)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nnn_k_recognised() {
        assert!(is_nnn_k("001-1"));
        assert!(is_nnn_k("999-12"));
        assert!(!is_nnn_k("01-1"));
        assert!(!is_nnn_k("rpm"));
        assert!(!is_nnn_k("001-"));
        assert!(!is_nnn_k("001"));
    }

    #[test]
    fn delimiter_sniff_picks_dominant() {
        assert_eq!(sniff_delimiter("a,b,c,d\n1,2,3,4"), ',');
        assert_eq!(sniff_delimiter("a;b;c;d\n1;2;3;4"), ';');
    }

    #[test]
    fn float_parses_eu_locale() {
        assert!((to_float("1.234,56", true) - 1234.56).abs() < 1e-9);
        assert!((to_float("3,5", true) - 3.5).abs() < 1e-9);
    }

    #[test]
    fn float_parses_us_locale() {
        assert!((to_float("3.5", false) - 3.5).abs() < 1e-9);
    }

    #[test]
    fn empty_string_is_nan() {
        assert!(to_float("", false).is_nan());
        assert!(to_float("   ", false).is_nan());
    }

    #[test]
    fn parse_str_minimal() {
        let csv = "Group A:,001\n\
                   TIME,,Engine speed,Injection quantity,Modulating piston\n\
                   s,,RPM,mg/H,V\n\
                   TIME,STAMP,001-1,001-2,001-3\n\
                   0.00,12:00:00.000,800,4.0,5.0\n\
                   0.25,12:00:00.250,810,4.1,5.0\n\
                   0.50,12:00:00.500,820,4.2,5.0\n";
        let log = parse_vcds_str(Path::new("inline.csv"), csv).expect("parse ok");
        assert_eq!(log.len(), 3);
        assert_eq!(log.data["rpm"], vec![800.0, 810.0, 820.0]);
        assert!((log.data["iq_actual"][1] - 4.1).abs() < 1e-9);
        assert!(log.groups.contains("001"));
    }
}
