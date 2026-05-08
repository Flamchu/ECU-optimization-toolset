//! DTC sidecar file ingest.
//!
//! VCDS DTCs do NOT come from a measuring block — they are produced by a
//! separate `01-DTC` scan. v4 ingest reads them from a sidecar text file
//! `<base>.dtc.txt` containing one DTC per line. Blank lines and lines
//! starting with `#` are ignored.
//!
//! Codes are validated against a permissive `P\d{4}` / `B\d{4}` /
//! `C\d{4}` / `U\d{4}` regex (hand-rolled, no `regex` crate). Anything
//! else is silently dropped — this is a permissive importer.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// Read a sidecar DTC file at `path` if it exists. Returns `Ok(vec)` with
/// zero or more codes; returns `Ok(empty)` if the file does not exist
/// (this is the normal case when no DTC scan was captured).
pub fn read_sidecar(path: impl AsRef<Path>) -> io::Result<Vec<String>> {
    let p = path.as_ref();
    if !p.exists() {
        return Ok(Vec::new());
    }
    let text = fs::read_to_string(p)?;
    Ok(parse_dtc_text(&text))
}

/// Convention: the sidecar lives next to the CSV with `.dtc.txt` appended
/// to the file stem. So `vcds_amf_pre_delete.csv` → `vcds_amf_pre_delete.dtc.txt`.
pub fn sidecar_path_for(csv_path: impl AsRef<Path>) -> PathBuf {
    let p = csv_path.as_ref();
    let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or("log");
    let parent = p.parent().unwrap_or_else(|| Path::new(""));
    parent.join(format!("{stem}.dtc.txt"))
}

/// Parse a DTC sidecar string into a deduplicated, ordered `Vec<String>`.
/// Lines beginning with `#` and blank lines are skipped. Codes are
/// uppercased.
pub fn parse_dtc_text(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut seen: Vec<String> = Vec::new();
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        // Take the first whitespace-separated token. Many VCDS exports
        // include a description after the code — we ignore everything
        // past the first token.
        let token = line.split_whitespace().next().unwrap_or("");
        if !is_obd_code(token) {
            continue;
        }
        let canonical = token.to_ascii_uppercase();
        if !seen.contains(&canonical) {
            seen.push(canonical.clone());
            out.push(canonical);
        }
    }
    out
}

/// Loose OBD-II code shape check: one letter from `PBCU` followed by
/// exactly 4 hex/decimal digits.
fn is_obd_code(s: &str) -> bool {
    if s.len() != 5 {
        return false;
    }
    let mut bytes = s.bytes();
    let prefix = bytes.next().unwrap_or(0).to_ascii_uppercase();
    if !matches!(prefix, b'P' | b'B' | b'C' | b'U') {
        return false;
    }
    bytes.all(|b| b.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn parses_one_code_per_line() {
        let txt = "P0401\nP0403\n";
        let codes = parse_dtc_text(txt);
        assert_eq!(codes, vec!["P0401", "P0403"]);
    }

    #[test]
    fn dedupes_and_uppercases() {
        let txt = "p0401\nP0401\nP0403\n";
        let codes = parse_dtc_text(txt);
        assert_eq!(codes, vec!["P0401", "P0403"]);
    }

    #[test]
    fn ignores_comments_and_blank_lines() {
        let txt = "# header line\n\nP0401\n\n# trailing\n";
        let codes = parse_dtc_text(txt);
        assert_eq!(codes, vec!["P0401"]);
    }

    #[test]
    fn ignores_descriptions_after_code() {
        let txt = "P0401  EGR insufficient flow\nP0403\tEGR solenoid circuit\n";
        let codes = parse_dtc_text(txt);
        assert_eq!(codes, vec!["P0401", "P0403"]);
    }

    #[test]
    fn rejects_malformed_codes() {
        let txt = "P040\nXX0401\nP04019\nfoo bar\n";
        let codes = parse_dtc_text(txt);
        assert!(codes.is_empty());
    }

    #[test]
    fn accepts_b_c_u_prefixes() {
        let txt = "B1234\nC5678\nU0100\n";
        let codes = parse_dtc_text(txt);
        assert_eq!(codes, vec!["B1234", "C5678", "U0100"]);
    }

    #[test]
    fn missing_sidecar_returns_empty() {
        let p = std::env::temp_dir().join("definitely_not_a_real_dtc_file__xyz.dtc.txt");
        // Make sure it doesn't exist.
        let _ = fs::remove_file(&p);
        let codes = read_sidecar(&p).unwrap();
        assert!(codes.is_empty());
    }

    #[test]
    fn reads_existing_sidecar() {
        let dir = std::env::temp_dir();
        let p = dir.join("ecu_shenanigans_test_dtc.dtc.txt");
        {
            let mut f = fs::File::create(&p).unwrap();
            f.write_all(b"P0401\nP0403\n").unwrap();
        }
        let codes = read_sidecar(&p).unwrap();
        assert_eq!(codes, vec!["P0401", "P0403"]);
        let _ = fs::remove_file(&p);
    }

    #[test]
    fn sidecar_path_appends_dtc_txt() {
        let p = sidecar_path_for("/tmp/vcds_amf_post_delete.csv");
        assert_eq!(p.file_name().unwrap(), "vcds_amf_post_delete.dtc.txt");
    }
}
