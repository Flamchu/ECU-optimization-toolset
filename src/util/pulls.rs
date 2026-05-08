//! WOT-pull detection.
//!
//! Per spec §4.2: a "pull" is `pedal >= 95 %` AND `RPM rising` AND
//! `duration >= 2 s`. The driver-pedal channel is `tps_pct` on AMF
//! (group 010-3). When TPS is missing, IQ-based fallback is used.

use crate::util::timebase::ResampledLog;

/// Pedal threshold (%) for "WOT".
pub const PEDAL_THRESHOLD_PCT: f64 = 95.0;
/// Minimum pull duration in seconds.
pub const MIN_DURATION_S: f64 = 2.0;
/// Centred-difference window (samples) for RPM-rising detection.
pub const RPM_RISING_WINDOW_SAMPLES: usize = 3;

/// One detected WOT pull.
#[derive(Debug, Clone)]
pub struct Pull {
    /// 1-based pull id.
    pub pull_id: u32,
    /// Inclusive index of pull start in the resampled log.
    pub i_start: usize,
    /// Exclusive index of pull end in the resampled log.
    pub i_end: usize,
    /// Time at start of pull (seconds).
    pub t_start: f64,
    /// Time at end of pull (seconds).
    pub t_end: f64,
    /// RPM at the first sample of the pull.
    pub rpm_start: f64,
    /// RPM at the last sample of the pull.
    pub rpm_end: f64,
}

impl Pull {
    /// Pull duration in seconds.
    pub fn duration_s(&self) -> f64 {
        self.t_end - self.t_start
    }
}

/// Identify WOT pulls in a uniformly-sampled log.
pub fn detect_pulls(log: &ResampledLog) -> Vec<Pull> {
    if log.is_empty() || !log.data.contains_key("rpm") {
        return Vec::new();
    }
    let rpm = match log.get("rpm") { Some(v) => v, None => return Vec::new() };
    let t = log.time.as_slice();
    let n = log.len();

    let wot = build_wot_mask(log, n);
    let Some(wot) = wot else { return Vec::new() };
    let rising = build_rising_mask(rpm, n);
    let candidate: Vec<bool> = wot.iter().zip(rising.iter())
        .map(|(a, b)| *a && *b)
        .collect();

    // Find runs of true.
    let mut runs: Vec<(usize, usize)> = Vec::new();
    let mut i = 0usize;
    while i < n {
        if candidate[i] {
            let mut j = i;
            while j < n && candidate[j] {
                j += 1;
            }
            runs.push((i, j));
            i = j;
        } else {
            i += 1;
        }
    }

    // Filter by duration and assemble Pull structs.
    let mut pulls: Vec<Pull> = Vec::new();
    for (s, e) in runs {
        if e == 0 || e <= s {
            continue;
        }
        let dur = t[e - 1] - t[s];
        if dur < MIN_DURATION_S {
            continue;
        }
        pulls.push(Pull {
            pull_id: 0,
            i_start: s,
            i_end: e,
            t_start: t[s],
            t_end: t[e - 1],
            rpm_start: rpm[s],
            rpm_end: rpm[e - 1],
        });
    }
    for (idx, p) in pulls.iter_mut().enumerate() {
        p.pull_id = (idx + 1) as u32;
    }
    pulls
}

fn build_wot_mask(log: &ResampledLog, n: usize) -> Option<Vec<bool>> {
    for cand in ["tps_pct", "pedal", "load_pct"] {
        if log.has(cand) {
            let v = log.get(cand)?;
            return Some(v.iter().map(|x| x.is_finite() && *x >= PEDAL_THRESHOLD_PCT).collect());
        }
    }
    // IQ-based fallback. Cutoff at 50 % of session-max IQ cleanly separates
    // WOT from idle without latching onto the peak — important because WOT
    // IQ naturally tapers from low to high RPM.
    for iq in ["iq_requested", "iq_actual"] {
        if log.has(iq) {
            let vals = log.get(iq)?;
            let max = vals.iter().cloned().filter(|x| x.is_finite())
                .fold(f64::NEG_INFINITY, f64::max);
            if !max.is_finite() {
                return None;
            }
            let cutoff = 0.5 * max;
            return Some(vals.iter().map(|x| x.is_finite() && *x >= cutoff).collect());
        }
    }
    let _ = n;
    None
}

fn build_rising_mask(rpm: &[f64], n: usize) -> Vec<bool> {
    let mut out = vec![false; n];
    let w = RPM_RISING_WINDOW_SAMPLES;
    if n > 2 * w {
        for i in w..(n - w) {
            let l = rpm[i - w];
            let r = rpm[i + w];
            out[i] = (r - l) > 0.0;
        }
    } else if n >= 2 {
        for i in 1..n {
            out[i] = rpm[i] > rpm[i - 1];
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn synth_log(rpm: Vec<f64>, tps: Vec<f64>) -> ResampledLog {
        let n = rpm.len();
        let dt = 0.2;
        let time: Vec<f64> = (0..n).map(|i| i as f64 * dt).collect();
        let mut data = BTreeMap::new();
        data.insert("rpm".to_string(), rpm);
        data.insert("tps_pct".to_string(), tps);
        ResampledLog { time, data }
    }

    #[test]
    fn empty_log_returns_no_pulls() {
        let log = ResampledLog { time: Vec::new(), data: BTreeMap::new() };
        assert!(detect_pulls(&log).is_empty());
    }

    #[test]
    fn synth_pull_detected() {
        let n = 60;
        let rpm: Vec<f64> = (0..n).map(|i| 1500.0 + (i as f64) * 50.0).collect();
        let tps = vec![100.0; n];
        let log = synth_log(rpm, tps);
        let pulls = detect_pulls(&log);
        assert_eq!(pulls.len(), 1);
        assert!(pulls[0].duration_s() >= MIN_DURATION_S);
    }

    #[test]
    fn iq_fallback_when_no_tps() {
        let n = 60;
        let rpm: Vec<f64> = (0..n).map(|i| 1500.0 + (i as f64) * 50.0).collect();
        let mut iq = vec![5.0; n];
        for x in iq.iter_mut().take(50).skip(10) { *x = 50.0; }
        let dt = 0.2;
        let time: Vec<f64> = (0..n).map(|i| i as f64 * dt).collect();
        let mut data = BTreeMap::new();
        data.insert("rpm".to_string(), rpm);
        data.insert("iq_requested".to_string(), iq);
        let log = ResampledLog { time, data };
        let pulls = detect_pulls(&log);
        assert!(!pulls.is_empty());
    }
}
