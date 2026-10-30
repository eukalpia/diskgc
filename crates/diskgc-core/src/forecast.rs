//! Growth forecast: an ordinary least-squares line through recent (time, used bytes) samples.
//! Deliberately simple — disk usage between cleanups is close to linear, and a model that can be
//! explained in one sentence is easier to trust than one that cannot.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Forecast {
    /// Bytes per hour; positive means the disk is filling.
    pub rate_bytes_per_hour: f64,
    pub window_hours: f64,
    pub samples: usize,
    /// Hours until `available` reaches the critical threshold at this rate; `None` if not growing.
    pub hours_to_critical: Option<f64>,
    /// Predicted available bytes in 24 hours.
    pub available_in_24h: u64,
}

/// `points` are `(hours relative to now, used bytes)`; hours are negative for the past.
pub fn slope(points: &[(f64, f64)]) -> Option<f64> {
    if points.len() < 3 {
        return None;
    }
    let (min, max) = points.iter().fold((f64::MAX, f64::MIN), |(lo, hi), p| {
        (lo.min(p.0), hi.max(p.0))
    });
    if max - min < 0.5 {
        return None;
    }
    let n = points.len() as f64;
    let (sx, sy) = points
        .iter()
        .fold((0.0, 0.0), |a, p| (a.0 + p.0, a.1 + p.1));
    let (mx, my) = (sx / n, sy / n);
    let sxx: f64 = points.iter().map(|p| (p.0 - mx).powi(2)).sum();
    if sxx == 0.0 {
        return None;
    }
    let sxy: f64 = points.iter().map(|p| (p.0 - mx) * (p.1 - my)).sum();
    Some(sxy / sxx)
}

pub fn forecast(
    points: &[(f64, f64)],
    window_hours: f64,
    available: u64,
    critical: u64,
) -> Option<Forecast> {
    let rate = slope(points)?;
    let growing = rate > 50.0 * 1024.0 * 1024.0; // ignore noise below 50 MB/h
    let hours_to_critical = if growing && available > critical {
        Some((available - critical) as f64 / rate)
    } else {
        None
    };
    let available_in_24h = if rate > 0.0 {
        (available as f64 - rate * 24.0).max(0.0) as u64
    } else {
        available
    };
    Some(Forecast {
        rate_bytes_per_hour: rate,
        window_hours,
        samples: points.len(),
        hours_to_critical,
        available_in_24h,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fits_a_line() {
        let pts: Vec<(f64, f64)> = (0..7)
            .map(|i| (-(6 - i) as f64, 100.0 + 2.0 * i as f64))
            .collect();
        let s = slope(&pts).unwrap();
        assert!((s - 2.0).abs() < 1e-9);
    }

    #[test]
    fn needs_three_points_and_half_an_hour() {
        assert!(slope(&[(-1.0, 1.0), (0.0, 2.0)]).is_none());
        assert!(slope(&[(-0.1, 1.0), (-0.05, 2.0), (0.0, 3.0)]).is_none());
    }

    #[test]
    fn predicts_time_to_critical() {
        let gb = 1024.0 * 1024.0 * 1024.0;
        let pts: Vec<(f64, f64)> = (0..7)
            .map(|i| (-(6 - i) as f64, 100.0 * gb + 5.0 * gb * i as f64))
            .collect();
        let f = forecast(&pts, 6.0, (110.0 * gb) as u64, (60.0 * gb) as u64).unwrap();
        assert!((f.hours_to_critical.unwrap() - 10.0).abs() < 0.01);
    }
}
