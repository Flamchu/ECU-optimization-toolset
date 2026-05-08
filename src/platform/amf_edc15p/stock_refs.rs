//! AMF stock reference values — defaults when no base map is loaded.
//!
//! All values per spec §2.2; treat each as ±5 %. The recommendation engine
//! falls back to these constants when the user has not supplied a real
//! basemap.

/// One row in the stock LDRXN ramp.
#[derive(Debug, Clone, Copy)]
pub struct BoostRefPoint {
    /// Engine speed at this anchor.
    pub rpm: i32,
    /// Stock target boost (mbar absolute).
    pub boost_mbar_abs: i32,
}

/// Boost target ramp — community-validated stock LDRXN curve for AMF.
pub const STOCK_BOOST_RAMP: &[BoostRefPoint] = &[
    BoostRefPoint { rpm: 1300, boost_mbar_abs: 1100 },
    BoostRefPoint { rpm: 2000, boost_mbar_abs: 1900 },
    BoostRefPoint { rpm: 2500, boost_mbar_abs: 1980 },
    BoostRefPoint { rpm: 3000, boost_mbar_abs: 2000 },
    BoostRefPoint { rpm: 3500, boost_mbar_abs: 1950 },
    BoostRefPoint { rpm: 4000, boost_mbar_abs: 1850 },
    BoostRefPoint { rpm: 4500, boost_mbar_abs: 1750 },
];

/// Peak IQ curve — what stock LDRXN + smoke limit deliver, indexed by RPM.
pub const STOCK_IQ_AT_RPM: &[(i32, f64)] = &[
    (1500, 38.0),
    (1750, 44.5),
    (2000, 44.5),
    (2500, 43.0),
    (3000, 41.0),
    (3500, 39.0),
    (4000, 37.0),
];

/// Stock SVBL (overboost cut) value.
pub const STOCK_SVBL_MBAR_ABS: i32 = 2300;

/// Stock SOI advance at 4000 rpm.
pub const STOCK_SOI_AT_4000_RPM_DEG_BTDC: f64 = 21.0;

/// Sea-level atmospheric pressure baseline.
pub const SEA_LEVEL_MBAR: i32 = 1013;

/// Stock fuelling produces ~4.4 Nm per mg/stroke of IQ at the flywheel.
pub const NM_PER_MG_IQ: f64 = 4.4;

/// Linear-interpolate the stock LDRXN curve. Returns mbar absolute.
pub fn stock_boost_at_rpm(rpm: f64) -> f64 {
    let ramp = STOCK_BOOST_RAMP;
    let first = &ramp[0];
    let last = &ramp[ramp.len() - 1];
    if rpm <= f64::from(first.rpm) {
        return f64::from(first.boost_mbar_abs);
    }
    if rpm >= f64::from(last.rpm) {
        return f64::from(last.boost_mbar_abs);
    }
    for w in ramp.windows(2) {
        let (l, r) = (&w[0], &w[1]);
        let lr = f64::from(l.rpm);
        let rr = f64::from(r.rpm);
        if (lr..=rr).contains(&rpm) {
            let frac = (rpm - lr) / (rr - lr);
            return f64::from(l.boost_mbar_abs)
                + frac * f64::from(r.boost_mbar_abs - l.boost_mbar_abs);
        }
    }
    f64::from(last.boost_mbar_abs)
}

/// Linear-interpolate the stock IQ curve. Returns mg/stroke.
pub fn stock_iq_at_rpm(rpm: f64) -> f64 {
    let table = STOCK_IQ_AT_RPM;
    let first = table[0];
    let last = table[table.len() - 1];
    if rpm <= f64::from(first.0) {
        return first.1;
    }
    if rpm >= f64::from(last.0) {
        return last.1;
    }
    for w in table.windows(2) {
        let (a, b) = (w[0], w[1]);
        let ar = f64::from(a.0);
        let br = f64::from(b.0);
        if (ar..=br).contains(&rpm) {
            let frac = (rpm - ar) / (br - ar);
            return a.1 + frac * (b.1 - a.1);
        }
    }
    last.1
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-6
    }

    #[test]
    fn boost_below_first_anchor_clamps_to_first() {
        assert!(approx(stock_boost_at_rpm(800.0), 1100.0));
    }

    #[test]
    fn boost_above_last_anchor_clamps_to_last() {
        assert!(approx(stock_boost_at_rpm(6000.0), 1750.0));
    }

    #[test]
    fn boost_interpolates_midway_between_anchors() {
        // halfway between 2000 (1900) and 2500 (1980) -> 1940
        assert!(approx(stock_boost_at_rpm(2250.0), 1940.0));
    }

    #[test]
    fn iq_table_clamps() {
        assert!(approx(stock_iq_at_rpm(500.0), 38.0));
        assert!(approx(stock_iq_at_rpm(9000.0), 37.0));
    }

    #[test]
    fn iq_interpolation_is_linear() {
        // halfway between 1500 (38.0) and 1750 (44.5) -> 41.25
        assert!(approx(stock_iq_at_rpm(1625.0), 41.25));
    }
}
