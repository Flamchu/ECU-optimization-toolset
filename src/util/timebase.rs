//! Resample VCDS logs to a uniform 5 Hz timebase.
//!
//! Per spec §4.1: 5 Hz (200 ms) is just above the ~3.5–4.5 sample/sec
//! ceiling VCDS achieves over KW1281 with one or two groups. We slightly
//! oversample then smooth — never extrapolate.
//!
//! Continuous channels are linearly interpolated; channels listed in
//! [`LOCF_CHANNELS`] use last-observation-carried-forward so a `1` does
//! not get smeared into a fractional value during interpolation.

use std::collections::BTreeMap;

use crate::ingest::VcdsLog;

/// Default resample rate (Hz).
pub const DEFAULT_RATE_HZ: f64 = 5.0;

/// Channels that should be LOCF-resampled rather than linearly
/// interpolated. Quasi-binary or strongly discontinuous channels go here;
/// interpolating them would lie about reality.
pub const LOCF_CHANNELS: &[&str] = &["egr_duty"];

/// Resampled log: a uniform time axis paired with channel buffers.
#[derive(Debug, Clone)]
pub struct ResampledLog {
    /// Uniform time axis in seconds.
    pub time: Vec<f64>,
    /// Channel name → resampled values (length matches `time`).
    pub data: BTreeMap<String, Vec<f64>>,
}

impl ResampledLog {
    /// Number of samples on the uniform grid.
    pub fn len(&self) -> usize {
        self.time.len()
    }

    /// Whether the resampled log has zero rows.
    pub fn is_empty(&self) -> bool {
        self.time.is_empty()
    }

    /// Borrow a channel by canonical name, if present.
    pub fn get(&self, name: &str) -> Option<&[f64]> {
        self.data.get(name).map(Vec::as_slice)
    }

    /// True if the channel exists and contains at least one finite sample.
    pub fn has(&self, name: &str) -> bool {
        self.data.get(name).is_some_and(|v| v.iter().any(|x| x.is_finite()))
    }
}

/// Resample `log` to `rate_hz`. Returns the original time axis if the
/// log has fewer than two samples.
pub fn resample_to_uniform(log: &VcdsLog, rate_hz: f64) -> ResampledLog {
    if log.time.len() < 2 {
        return ResampledLog {
            time: log.time.clone(),
            data: log.data.clone(),
        };
    }

    let t_start = log.time[0];
    let t_end = log.time[log.time.len() - 1];
    let dt = 1.0 / rate_hz;
    let mut new_t = Vec::new();
    let mut t = t_start;
    while t <= t_end + dt / 2.0 {
        new_t.push(t);
        t += dt;
    }

    let mut out: BTreeMap<String, Vec<f64>> = BTreeMap::new();
    for (col, vals) in &log.data {
        let resampled = if LOCF_CHANNELS.contains(&col.as_str()) {
            locf_resample(&log.time, vals, &new_t)
        } else {
            linear_resample(&log.time, vals, &new_t)
        };
        out.insert(col.clone(), resampled);
    }

    ResampledLog { time: new_t, data: out }
}

/// Linearly interpolate `(t, v)` to `new_t`. Keeps NaN-only channels as
/// NaN; otherwise restricts to finite samples for interpolation.
fn linear_resample(t: &[f64], v: &[f64], new_t: &[f64]) -> Vec<f64> {
    let finite: Vec<(f64, f64)> = t.iter().zip(v.iter())
        .filter(|(_, &y)| y.is_finite())
        .map(|(&x, &y)| (x, y))
        .collect();
    if finite.is_empty() {
        return vec![f64::NAN; new_t.len()];
    }
    let xs: Vec<f64> = finite.iter().map(|(x, _)| *x).collect();
    let ys: Vec<f64> = finite.iter().map(|(_, y)| *y).collect();
    new_t.iter().map(|&q| interp1(&xs, &ys, q)).collect()
}

/// LOCF (last observation carried forward) resampling. Suitable for
/// status / quasi-binary channels.
fn locf_resample(t: &[f64], v: &[f64], new_t: &[f64]) -> Vec<f64> {
    let finite: Vec<(f64, f64)> = t.iter().zip(v.iter())
        .filter(|(_, &y)| y.is_finite())
        .map(|(&x, &y)| (x, y))
        .collect();
    if finite.is_empty() {
        return vec![f64::NAN; new_t.len()];
    }
    let xs: Vec<f64> = finite.iter().map(|(x, _)| *x).collect();
    let ys: Vec<f64> = finite.iter().map(|(_, y)| *y).collect();
    new_t.iter().map(|&q| {
        let idx = match xs.binary_search_by(|x| x.partial_cmp(&q).unwrap_or(std::cmp::Ordering::Equal)) {
            Ok(i) => i,
            Err(0) => 0,
            Err(i) => i - 1,
        };
        ys[idx.min(ys.len() - 1)]
    }).collect()
}

/// Linear interpolation of `(xs, ys)` at `q`. `xs` must be ascending.
/// Clamps to endpoints outside the range.
fn interp1(xs: &[f64], ys: &[f64], q: f64) -> f64 {
    if q <= xs[0] {
        return ys[0];
    }
    if q >= xs[xs.len() - 1] {
        return ys[ys.len() - 1];
    }
    let pos = xs.binary_search_by(|x| x.partial_cmp(&q).unwrap_or(std::cmp::Ordering::Equal));
    let i = match pos {
        Ok(i) => return ys[i],
        Err(i) => i,
    };
    let (x0, x1) = (xs[i - 1], xs[i]);
    let (y0, y1) = (ys[i - 1], ys[i]);
    let frac = (q - x0) / (x1 - x0);
    y0 + frac * (y1 - y0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interp1_basic() {
        let xs = vec![0.0, 1.0, 2.0];
        let ys = vec![0.0, 10.0, 30.0];
        assert!((interp1(&xs, &ys, 0.5) - 5.0).abs() < 1e-9);
        assert!((interp1(&xs, &ys, 1.5) - 20.0).abs() < 1e-9);
        assert!((interp1(&xs, &ys, -1.0) - 0.0).abs() < 1e-9);
        assert!((interp1(&xs, &ys, 5.0) - 30.0).abs() < 1e-9);
    }

    #[test]
    fn linear_resample_grows_grid() {
        let t = vec![0.0, 1.0];
        let v = vec![0.0, 10.0];
        let new_t = vec![0.0, 0.5, 1.0];
        let r = linear_resample(&t, &v, &new_t);
        assert_eq!(r, vec![0.0, 5.0, 10.0]);
    }

    #[test]
    fn locf_resample_holds() {
        let t = vec![0.0, 1.0, 2.0];
        let v = vec![1.0, 2.0, 3.0];
        let new_t = vec![0.0, 0.5, 1.0, 1.5, 2.0];
        let r = locf_resample(&t, &v, &new_t);
        assert_eq!(r, vec![1.0, 1.0, 2.0, 2.0, 3.0]);
    }
}
