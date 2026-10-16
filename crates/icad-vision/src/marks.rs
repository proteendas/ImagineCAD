// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Proteen Das

//! Drafting conventions in a traced sketch: dashed hidden lines and
//! chain-dotted centre lines (rows of short collinear strokes), and section
//! hatching (a regular family of short parallel strokes).

use crate::fit::Geo;
use icad_core::geom::*;

/// A dashed line found among the strokes.
#[derive(Clone, Debug)]
pub struct Dashed {
    /// Indices of the dashes.
    pub members: Vec<usize>,
    pub a: V2,
    pub b: V2,
    /// Long and short dashes alternate (centre line) rather than equal dashes (hidden line).
    pub center: bool,
}

/// A family of hatch strokes.
#[derive(Clone, Debug)]
pub struct HatchFamily {
    pub members: Vec<usize>,
    /// A point inside the hatched region (on a middle stroke).
    pub seed: V2,
    /// Midpoints of every stroke (the family may cover several regions).
    pub points: Vec<V2>,
    /// Stroke direction (radians, 0..π).
    pub angle: f64,
    /// Distance between strokes (px).
    pub spacing: f64,
}

fn line(g: &Geo) -> Option<(V2, V2)> {
    if let Geo::Line(a, b) = *g {
        Some((a, b))
    } else {
        None
    }
}

/// Undirected angle 0..π.
fn dir_angle(a: V2, b: V2) -> f64 {
    let t = (b - a).angle();
    t.rem_euclid(std::f64::consts::PI)
}

fn angle_diff(a: f64, b: f64) -> f64 {
    let d = (a - b).rem_euclid(std::f64::consts::PI);
    d.min(std::f64::consts::PI - d)
}

/// Rows of three or more short collinear strokes with regular gaps.
pub fn find_dashed(geos: &[Geo], tol: f64, diag: f64) -> Vec<Dashed> {
    let short: Vec<usize> =
        (0..geos.len()).filter(|i| line(&geos[*i]).is_some_and(|(a, b)| (0.004 * diag..0.06 * diag).contains(&a.dist(b)))).collect();
    let mut used = vec![false; geos.len()];
    let mut order = short.clone();
    order.sort_by(|i, j| {
        let len = |k: &usize| line(&geos[*k]).map(|(a, b)| a.dist(b)).unwrap_or(0.0);
        len(j).total_cmp(&len(i))
    });
    let mut out = vec![];
    for &seed in &order {
        if used[seed] {
            continue;
        }
        let (sa, sb) = line(&geos[seed]).unwrap();
        let d = (sb - sa).norm();
        let n = d.perp();
        let ang = dir_angle(sa, sb);
        // Collinear short strokes.
        let mut row: Vec<(f64, f64, usize)> = short
            .iter()
            .filter(|k| !used[**k])
            .filter_map(|&k| {
                let (a, b) = line(&geos[k])?;
                let on = (a - sa).dot(n).abs() < 0.6 * tol && (b - sa).dot(n).abs() < 0.6 * tol;
                (on && angle_diff(dir_angle(a, b), ang) < 5f64.to_radians()).then(|| {
                    let (ta, tb) = ((a - sa).dot(d), (b - sa).dot(d));
                    (ta.min(tb), ta.max(tb), k)
                })
            })
            .collect();
        row.sort_by(|p, q| p.0.total_cmp(&q.0));
        // Split into runs at gaps that are too long (or overlaps: not dashes).
        let mut runs: Vec<Vec<(f64, f64, usize)>> = vec![];
        for item in row {
            match runs.last_mut() {
                Some(run) => {
                    let last = run[run.len() - 1];
                    let mean = run.iter().map(|r| r.1 - r.0).sum::<f64>() / run.len() as f64;
                    let gap = item.0 - last.1;
                    // A real gap is clearly empty: wider than a stroke junction.
                    if gap > 0.5 * tol && gap < 2.5 * mean.max(item.1 - item.0) {
                        run.push(item);
                    } else {
                        runs.push(vec![item]);
                    }
                }
                None => runs.push(vec![item]),
            }
        }
        for run in runs.into_iter().filter(|r| r.len() >= 3 && r.iter().any(|x| x.2 == seed)) {
            let lens: Vec<f64> = run.iter().map(|r| r.1 - r.0).collect();
            let alternating = lens.windows(2).filter(|w| w[0].max(w[1]) > 2.2 * w[0].min(w[1])).count();
            let center = alternating * 10 >= (lens.len() - 1) * 4;
            for r in &run {
                used[r.2] = true;
            }
            out.push(Dashed { members: run.iter().map(|r| r.2).collect(), a: sa + d * run[0].0, b: sa + d * run[run.len() - 1].1, center });
        }
    }
    out
}

/// Families of four or more short parallel strokes at a regular spacing,
/// running from edge to edge (section hatching).
pub fn find_hatching(geos: &[Geo], skip: &[bool], tol: f64, diag: f64) -> Vec<HatchFamily> {
    // Drafting hatch runs at 45° or 135° (handwriting slants steeper).
    let hatch_like =
        |a: f64| angle_diff(a, std::f64::consts::FRAC_PI_4).min(angle_diff(a, 3.0 * std::f64::consts::FRAC_PI_4)) < 17f64.to_radians();
    let cand: Vec<(usize, V2, V2, f64)> = (0..geos.len())
        .filter(|i| !skip[*i])
        .filter_map(|i| {
            let (a, b) = line(&geos[i])?;
            let ang = dir_angle(a, b);
            ((0.008 * diag..0.45 * diag).contains(&a.dist(b)) && hatch_like(ang)).then_some((i, a, b, ang))
        })
        .collect();
    let mut taken = vec![false; cand.len()];
    let mut out = vec![];
    for s in 0..cand.len() {
        if taken[s] {
            continue;
        }
        // Lines of the same direction, grown by neighbourhood.
        let ang = cand[s].3;
        let d = V2::polar(1.0, ang);
        let n = d.perp();
        let mut group = vec![s];
        let mut in_group = vec![false; cand.len()];
        in_group[s] = true;
        let mut k = 0;
        while k < group.len() {
            let (_, a, b, _) = cand[group[k]];
            for (j, c) in cand.iter().enumerate() {
                if taken[j] || in_group[j] || angle_diff(c.3, ang) > 6f64.to_radians() {
                    continue;
                }
                let off = ((c.1 + c.2) * 0.5 - (a + b) * 0.5).dot(n).abs();
                // Overlapping along the stroke direction, and a near neighbour across.
                let (t0, t1) = ((a - c.1).dot(d), (b - c.1).dot(d));
                let (u0, u1) = (0.0f64.min((c.2 - c.1).dot(d)), 0.0f64.max((c.2 - c.1).dot(d)));
                let overlap = t0.max(t1) > u0 && t0.min(t1) < u1;
                if overlap && off < 0.08 * diag && off > 0.5 * tol {
                    group.push(j);
                    in_group[j] = true;
                }
            }
            k += 1;
        }
        // Whether or not it is hatching, these strokes have been judged.
        for &g in &group {
            taken[g] = true;
        }
        if group.len() < 4 {
            continue;
        }
        let mut offs: Vec<(f64, usize)> = group.iter().map(|&g| (((cand[g].1 + cand[g].2) * 0.5).dot(n), g)).collect();
        offs.sort_by(|p, q| p.0.total_cmp(&q.0));
        // Pieces of one hatch line (broken by tracing) share an offset.
        let mut lines: Vec<f64> = vec![];
        for (o, _) in &offs {
            if lines.last().is_none_or(|l| o - l > 0.35 * tol) {
                lines.push(*o);
            }
        }
        if lines.len() < 4 {
            continue;
        }
        let diffs: Vec<f64> = lines.windows(2).map(|w| w[1] - w[0]).collect();
        let mut sorted = diffs.clone();
        sorted.sort_by(f64::total_cmp);
        let sp = sorted[sorted.len() / 2];
        let regular = diffs.iter().filter(|x| (0.5 * sp..1.6 * sp).contains(*x)).count();
        if sp < 0.6 * tol || regular * 10 < diffs.len() * 7 {
            continue;
        }
        // Hatch strokes run from edge to edge: most of them end on other strokes.
        let members: Vec<usize> = group.iter().map(|g| cand[*g].0).collect();
        // On real edges, not on tiny strokes.
        let touches = |p: V2| {
            geos.iter().enumerate().any(|(k, g)| {
                !members.contains(&k)
                    && g.length() >= 2.0 * tol
                    && match *g {
                        Geo::Line(a, b) => dist_point_segment(p, a, b) < 1.5 * tol,
                        Geo::Circle(c, r) => (p.dist(c) - r).abs() < 1.5 * tol,
                        Geo::Arc { c, r, start, end } => (p.dist(c) - r).abs() < 1.5 * tol && angle_on_arc((p - c).angle(), start, end),
                    }
            })
        };
        // Judge whole hatch lines: the outer ends of the pieces sharing an offset.
        let mut whole: Vec<(f64, V2, f64, V2)> = vec![];
        for (o, g) in &offs {
            let (a, b) = (cand[*g].1, cand[*g].2);
            let (ta, tb) = (a.dot(d), b.dot(d));
            let (lo, hi) = if ta <= tb { ((ta, a), (tb, b)) } else { ((tb, b), (ta, a)) };
            match whole.iter_mut().find(|w| ((w.1 + w.3) * 0.5).dot(n) - o < 0.35 * tol && o - ((w.1 + w.3) * 0.5).dot(n) < 0.35 * tol) {
                Some(w) => {
                    if lo.0 < w.0 {
                        (w.0, w.1) = lo;
                    }
                    if hi.0 > w.2 {
                        (w.2, w.3) = hi;
                    }
                }
                None => whole.push((lo.0, lo.1, hi.0, hi.1)),
            }
        }
        let bounded = whole.iter().filter(|w| touches(w.1) && touches(w.3)).count();
        if bounded * 10 < whole.len() * 6 {
            continue;
        }
        for &g in &group {
            taken[g] = true;
        }
        let mid = offs[offs.len() / 2].1;
        out.push(HatchFamily {
            members: group.iter().map(|g| cand[*g].0).collect(),
            seed: (cand[mid].1 + cand[mid].2) * 0.5,
            points: group.iter().map(|g| (cand[*g].1 + cand[*g].2) * 0.5).collect(),
            angle: ang,
            spacing: sp,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dashes_and_hatching_are_recognised() {
        let mut g = vec![];
        // Hidden line: equal dashes along y = 50.
        for i in 0..6 {
            let x = 10.0 + i as f64 * 14.0;
            g.push(Geo::Line(V2::new(x, 50.0), V2::new(x + 9.0, 50.0)));
        }
        // Centre line: long, short, long… along y = 80.
        let mut x = 10.0;
        for i in 0..7 {
            let len = if i % 2 == 0 { 24.0 } else { 4.0 };
            g.push(Geo::Line(V2::new(x, 80.0), V2::new(x + len, 80.0)));
            x += len + 4.0;
        }
        // Hatching: 45° strokes 6 apart in the corner of a box.
        g.push(Geo::Line(V2::new(200.0, 200.0), V2::new(260.0, 200.0)));
        g.push(Geo::Line(V2::new(200.0, 200.0), V2::new(200.0, 260.0)));
        let base = g.len();
        for i in 1..9 {
            let o = i as f64 * 6.0;
            g.push(Geo::Line(V2::new(200.0 + o, 200.0), V2::new(200.0, 200.0 + o)));
        }
        let dashed = find_dashed(&g, 3.0, 1000.0);
        assert_eq!(dashed.len(), 2, "{dashed:?}");
        let hidden = dashed.iter().find(|d| !d.center).unwrap();
        assert_eq!(hidden.members.len(), 6);
        assert!((hidden.a.x - 10.0).abs() < 1e-9 && (hidden.b.x - 89.0).abs() < 1e-9);
        assert!(dashed.iter().any(|d| d.center && d.members.len() == 7));
        let skip = vec![false; g.len()];
        let h = find_hatching(&g, &skip, 3.0, 1000.0);
        assert_eq!(h.len(), 1, "{h:?}");
        assert_eq!(h[0].members.len(), 8);
        assert!(h[0].members.iter().all(|m| *m >= base));
        assert!((h[0].spacing - 6.0 * std::f64::consts::FRAC_1_SQRT_2).abs() < 0.1, "{}", h[0].spacing);
    }
}
