//! Grep gate: certain feature names must NEVER appear anywhere in the
//! repo (source, tests, docs, README). This is a contractual standing
//! check; failure means a banned feature name has crept back in.
//!
//! The forbidden tokens are built at runtime so this file does not
//! self-trigger when the gate scans itself.

use std::path::Path;

/// Forbidden tokens, built at runtime so they do not appear literally
/// in the source of this file. The boolean controls match strength:
/// `true` = whole-word only (case-insensitive); `false` = substring match.
fn forbidden_needles() -> Vec<(String, bool)> {
    vec![
        // Multi-character feature names — substring match is fine.
        (format!("{}-{}", "FLAT", "FOOT"), false),
        (format!("{}{}", "FLAT", "FOOT"), false),
        (format!("{}-{}", "NO", "LIFT"), false),
        (format!("{}{}", "NO", "LIFT"), false),
        // Three-letter abbreviation — word boundary so we don't trip on
        // English words like "diffs", "offset", etc.
        (format!("{}{}", "FF", "S"), true),
    ]
}

fn walk(dir: &Path, hits: &mut Vec<(String, usize, String, String)>, needles: &[(String, bool)]) {
    if !dir.exists() { return; }
    for entry in std::fs::read_dir(dir).unwrap().flatten() {
        let path = entry.path();
        if path.is_dir() {
            let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
            if name == "target" || name == "dev" || name.starts_with('.') { continue; }
            walk(&path, hits, needles);
        } else {
            walk_file(&path, hits, needles);
        }
    }
}

fn walk_file(path: &Path, hits: &mut Vec<(String, usize, String, String)>, needles: &[(String, bool)]) {
    let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("");
    if !matches!(ext, "rs" | "md" | "toml" | "yml" | "yaml") { return; }
    let Ok(text) = std::fs::read_to_string(path) else { return };
    for (i, line) in text.lines().enumerate() {
        let upper = line.to_ascii_uppercase();
        for (needle, word_only) in needles {
            let found = if *word_only {
                contains_as_word(&upper, needle)
            } else {
                upper.contains(needle)
            };
            if found {
                hits.push((path.display().to_string(), i + 1, needle.clone(), line.to_string()));
            }
        }
    }
}

/// Whether `haystack` contains `needle` as a stand-alone token. Token
/// boundaries are non-alphanumeric / non-`_` characters and the string
/// edges. Hand-rolled to avoid pulling in `regex`.
fn contains_as_word(haystack: &str, needle: &str) -> bool {
    let bytes = haystack.as_bytes();
    let needle_bytes = needle.as_bytes();
    if needle_bytes.is_empty() { return false; }
    let mut start = 0;
    while start + needle_bytes.len() <= bytes.len() {
        if let Some(off) = haystack[start..].find(needle) {
            let abs = start + off;
            let before = if abs == 0 { None } else { Some(bytes[abs - 1]) };
            let after_idx = abs + needle_bytes.len();
            let after = if after_idx >= bytes.len() { None } else { Some(bytes[after_idx]) };
            let boundary_before = before.map_or(true, |b| !is_word_byte(b));
            let boundary_after = after.map_or(true, |b| !is_word_byte(b));
            if boundary_before && boundary_after {
                return true;
            }
            start = abs + 1;
        } else {
            break;
        }
    }
    false
}

fn is_word_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

#[test]
fn no_banned_feature_names_anywhere() {
    let needles = forbidden_needles();
    let mut hits: Vec<(String, usize, String, String)> = Vec::new();
    for root in ["src", "tests"].iter() {
        walk(Path::new(root), &mut hits, &needles);
    }
    for f in ["README.md", "CHANGELOG.md", "Cargo.toml"].iter() {
        walk_file(Path::new(f), &mut hits, &needles);
    }
    walk(Path::new("docs"), &mut hits, &needles);
    if !hits.is_empty() {
        let report: Vec<String> = hits.iter()
            .map(|(p, l, n, t)| format!("{p}:{l} contains forbidden token '{n}': {t}"))
            .collect();
        panic!("Banned feature names found in repository:\n{}", report.join("\n"));
    }
}

#[test]
fn contains_as_word_finds_standalone_token() {
    assert!(contains_as_word("HELLO FOO BAR", "FOO"));
    assert!(contains_as_word("FOO", "FOO"));
    assert!(contains_as_word("PREFIX-FOO-SUFFIX", "FOO"));
}

#[test]
fn contains_as_word_skips_substring_match() {
    // Build the test needle at runtime so this file does not contain a
    // literal occurrence of the banned token.
    let needle = format!("{}{}", "FF", "S");
    let inside_word = format!("DI{}HELLO", needle); // word with the needle as a substring
    let surrounded = "OFFSETOK".to_string(); // unrelated, but contains the substring inside another word
    assert!(!contains_as_word(&inside_word, &needle));
    assert!(!contains_as_word(&surrounded, &needle));
    assert!(!contains_as_word("FOOBAR", "FOO"));
    let standalone = format!("DI{} {} BU{}", needle, needle, needle);
    assert!(contains_as_word(&standalone, &needle));
}
