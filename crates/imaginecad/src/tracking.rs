// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Proteen Das

//! Object snap tracking (OTRACK): rest the cursor on an object snap point to
//! acquire it; the cursor then aligns horizontally, vertically (or at polar
//! angles) with acquired points, and at the intersection of two alignments.

use icad_core::geom::*;
use std::time::{Duration, Instant};

/// Time the cursor must rest on a snap point to acquire it.
const DWELL: Duration = Duration::from_millis(450);
const MAX_POINTS: usize = 7;

#[derive(Default)]
pub struct Tracker {
    pub points: Vec<V2>,
    dwell: Option<(V2, Instant, bool)>,
}

impl Tracker {
    /// Feed the current object snap (if any). Returns true while waiting on a
    /// dwell, so the caller keeps repainting.
    pub fn update(&mut self, snap: Option<V2>, now: Instant) -> bool {
        let Some(p) = snap else {
            self.dwell = None;
            return false;
        };
        match &mut self.dwell {
            Some((q, since, done)) if q.dist(p) < 1e-9 => {
                if !*done && now.duration_since(*since) >= DWELL {
                    *done = true;
                    // Resting again on an acquired point releases it.
                    if let Some(i) = self.points.iter().position(|t| t.dist(p) < 1e-9) {
                        self.points.remove(i);
                    } else {
                        self.points.push(p);
                        if self.points.len() > MAX_POINTS {
                            self.points.remove(0);
                        }
                    }
                }
                !*done
            }
            _ => {
                self.dwell = Some((p, now, false));
                true
            }
        }
    }

    pub fn clear(&mut self) {
        self.points.clear();
        self.dwell = None;
    }
}

/// A tracking line through `from` at `angle` (degrees).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ray {
    pub from: V2,
    pub angle: f64,
}

fn dir(a: f64) -> V2 {
    V2::polar(1.0, a.to_radians())
}

/// Align `p` with the tracked points. `tol` is the capture distance in
/// drawing units; `polar_step` adds polar angles besides 0° and 90°.
/// Returns the aligned point and the tracking lines it lies on.
pub fn align(points: &[V2], p: V2, tol: f64, polar_step: Option<f64>) -> Option<(V2, Vec<Ray>)> {
    let mut angles = vec![0.0, 90.0];
    if let Some(step) = polar_step.filter(|s| *s >= 1.0) {
        let mut a = step;
        while a < 180.0 - 1e-9 {
            if (a - 90.0).abs() > 1e-9 {
                angles.push(a);
            }
            a += step;
        }
    }
    // Candidate lines with the cursor's distance to them.
    let mut cands: Vec<(Ray, f64)> = vec![];
    for t in points {
        if t.dist(p) < tol {
            continue;
        }
        for &a in &angles {
            let d = dir(a);
            let off = p - *t;
            let dist = (off - d * off.dot(d)).len();
            if dist <= tol {
                cands.push((Ray { from: *t, angle: a }, dist));
            }
        }
    }
    cands.sort_by(|a, b| a.1.total_cmp(&b.1));
    let (best, _) = *cands.first()?;
    // Two lines from different points: snap to their intersection.
    for (other, _) in cands.iter().skip(1) {
        if other.from.dist(best.from) < 1e-9 || ((other.angle - best.angle) % 180.0).abs() < 1e-9 {
            continue;
        }
        if let Some((x, _, _)) = line_line(best.from, best.from + dir(best.angle), other.from, other.from + dir(other.angle)) {
            if x.dist(p) <= tol * 2.0 {
                return Some((x, vec![best, *other]));
            }
        }
    }
    let d = dir(best.angle);
    Some((best.from + d * (p - best.from).dot(d), vec![best]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn acquisition_and_alignment() {
        let mut tr = Tracker::default();
        let t0 = Instant::now();
        let a = V2::new(0.0, 0.0);
        assert!(tr.update(Some(a), t0));
        assert!(tr.points.is_empty());
        tr.update(Some(a), t0 + Duration::from_millis(500));
        assert_eq!(tr.points, vec![a]);
        // Moving away and back releases it after another rest.
        tr.update(None, t0 + Duration::from_millis(600));
        tr.update(Some(a), t0 + Duration::from_millis(700));
        tr.update(Some(a), t0 + Duration::from_millis(1300));
        assert!(tr.points.is_empty());

        let b = V2::new(50.0, 30.0);
        // Horizontal alignment with a.
        let (q, rays) = align(&[a], V2::new(40.0, 0.7), 1.0, None).unwrap();
        assert_eq!((q, rays.len()), (V2::new(40.0, 0.0), 1));
        // Vertical from b and horizontal from a meet at (50, 0).
        let (q, rays) = align(&[a, b], V2::new(49.4, 0.6), 1.0, None).unwrap();
        assert_eq!((q, rays.len()), (V2::new(50.0, 0.0), 2));
        // Polar 45° from a.
        let (q, _) = align(&[a], V2::new(20.3, 19.8), 1.0, Some(45.0)).unwrap();
        assert!((q.x - q.y).abs() < 1e-9);
        assert!(align(&[a], V2::new(20.0, 10.0), 1.0, None).is_none());
    }
}
