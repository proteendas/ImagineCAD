// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Proteen Das

//! "Beautification": turns wobbly hand-drawn primitives into clean CAD
//! geometry. Snaps angles, merges collinear strokes, aligns coordinates,
//! welds endpoints and closes T-junctions.

use crate::fit::Geo;
use icad_core::geom::*;

#[derive(Clone, Copy, Debug)]
pub struct Tol {
    /// General alignment / welding tolerance (px).
    pub align: f64,
    /// Tight tolerance for merging parallel strokes (px).
    pub tight: f64,
    /// Lines shorter than this with a free end are noise (px).
    pub min_len: f64,
    /// Angle snapping threshold (degrees).
    pub snap_deg: f64,
}

pub fn is_h(g: &Geo) -> bool {
    matches!(g, Geo::Line(a, b) if (a.y - b.y).abs() < 1e-9 && (a.x - b.x).abs() > 1e-9)
}
pub fn is_v(g: &Geo) -> bool {
    matches!(g, Geo::Line(a, b) if (a.x - b.x).abs() < 1e-9 && (a.y - b.y).abs() > 1e-9)
}

pub fn snap_angles(geos: &mut [Geo], snap_deg: f64) {
    for g in geos.iter_mut() {
        if let Geo::Line(a, b) = g {
            let d = *b - *a;
            let ang = deg(d.angle()).rem_euclid(180.0);
            let m = a.mid(*b);
            let l = d.len();
            if ang < snap_deg || ang > 180.0 - snap_deg {
                a.y = m.y;
                b.y = m.y;
            } else if (ang - 90.0).abs() < snap_deg {
                a.x = m.x;
                b.x = m.x;
            } else {
                for target in [45.0f64, 135.0] {
                    if (ang - target).abs() < snap_deg * 0.5 {
                        let dir = V2::polar(1.0, rad(target));
                        let s = if d.dot(dir) >= 0.0 { 1.0 } else { -1.0 };
                        *a = m - dir * (s * l / 2.0);
                        *b = m + dir * (s * l / 2.0);
                    }
                }
            }
        }
    }
}

/// Merge overlapping / nearly touching collinear lines.
pub fn merge_collinear(geos: &mut Vec<Geo>, tight: f64, gap: f64) {
    loop {
        let mut merged = false;
        'outer: for i in 0..geos.len() {
            let Geo::Line(a1, b1) = geos[i] else { continue };
            for j in i + 1..geos.len() {
                let Geo::Line(a2, b2) = geos[j] else { continue };
                if let Some(m) = try_merge((a1, b1), (a2, b2), tight, gap) {
                    geos[i] = m;
                    geos.remove(j);
                    merged = true;
                    break 'outer;
                }
            }
        }
        if !merged {
            break;
        }
    }
}

fn try_merge(l1: (V2, V2), l2: (V2, V2), tight: f64, gap: f64) -> Option<Geo> {
    let len1 = l1.0.dist(l1.1);
    let len2 = l2.0.dist(l2.1);
    if len1 < 1e-9 || len2 < 1e-9 {
        return None;
    }
    let (base, other, lb, lo) = if len1 >= len2 { (l1, l2, len1, len2) } else { (l2, l1, len2, len1) };
    let d = (base.1 - base.0) / lb;
    let d2 = (other.1 - other.0) / lo;
    if d.cross(d2).abs() > 3f64.to_radians().sin() {
        return None;
    }
    let n = d.perp();
    let off_o0 = (other.0 - base.0).dot(n);
    let off_o1 = (other.1 - base.0).dot(n);
    if off_o0.abs() > tight || off_o1.abs() > tight {
        return None;
    }
    let t0 = (other.0 - base.0).dot(d);
    let t1 = (other.1 - base.0).dot(d);
    let (omin, omax) = (t0.min(t1), t0.max(t1));
    if omin > lb + gap || omax < -gap {
        return None;
    }
    let lo_t = omin.min(0.0);
    let hi_t = omax.max(lb);
    let off = (off_o0 + off_o1) / 2.0 * lo / (lb + lo);
    let a = base.0 + d * lo_t + n * off;
    let b = base.0 + d * hi_t + n * off;
    let horizontal = (l1.0.y - l1.1.y).abs() < 1e-9 && (l2.0.y - l2.1.y).abs() < 1e-9;
    let vertical = (l1.0.x - l1.1.x).abs() < 1e-9 && (l2.0.x - l2.1.x).abs() < 1e-9;
    Some(if horizontal {
        let y = (l1.0.y * len1 + l2.0.y * len2) / (len1 + len2);
        Geo::Line(V2::new(a.x.min(b.x), y), V2::new(a.x.max(b.x), y))
    } else if vertical {
        let x = (l1.0.x * len1 + l2.0.x * len2) / (len1 + len2);
        Geo::Line(V2::new(x, a.y.min(b.y)), V2::new(x, a.y.max(b.y)))
    } else {
        Geo::Line(a, b)
    })
}

/// 1-D clustering: returns (representative, members) groups.
pub fn cluster_1d(mut vals: Vec<(f64, f64, bool)>, tol: f64) -> Vec<(f64, Vec<f64>)> {
    // (value, weight, strong)
    vals.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut out: Vec<(f64, Vec<f64>)> = Vec::new();
    let mut group: Vec<(f64, f64, bool)> = Vec::new();
    let flush = |group: &mut Vec<(f64, f64, bool)>, out: &mut Vec<(f64, Vec<f64>)>| {
        if group.is_empty() {
            return;
        }
        let strong: Vec<_> = group.iter().filter(|g| g.2).collect();
        let pool: Vec<&(f64, f64, bool)> = if strong.is_empty() { group.iter().collect() } else { strong };
        let wsum: f64 = pool.iter().map(|g| g.1).sum();
        let rep = pool.iter().map(|g| g.0 * g.1).sum::<f64>() / wsum.max(1e-12);
        out.push((rep, group.iter().map(|g| g.0).collect()));
        group.clear();
    };
    for v in vals {
        if let (Some(last), Some(first)) = (group.last(), group.first()) {
            if v.0 - last.0 > tol || v.0 - first.0 > 2.0 * tol {
                flush(&mut group, &mut out);
            }
        }
        group.push(v);
    }
    flush(&mut group, &mut out);
    out
}

fn snapper(groups: &[(f64, Vec<f64>)]) -> impl Fn(f64) -> f64 + '_ {
    move |v: f64| {
        for (rep, members) in groups {
            if members.len() > 1 && members.iter().any(|m| (m - v).abs() < 1e-9) {
                return *rep;
            }
        }
        v
    }
}

/// Align x and y coordinates of axis-aligned features and endpoints.
pub fn align_axes(geos: &mut [Geo], tol: f64) {
    for axis in 0..2 {
        let get = |p: V2| if axis == 0 { p.x } else { p.y };
        let mut vals = Vec::new();
        for g in geos.iter() {
            match *g {
                Geo::Line(a, b) => {
                    let strong = if axis == 0 { is_v(g) } else { is_h(g) };
                    if strong {
                        vals.push((get(a), a.dist(b), true));
                    } else {
                        vals.push((get(a), 1.0, false));
                        vals.push((get(b), 1.0, false));
                    }
                }
                Geo::Circle(c, _) => vals.push((get(c), 3.0, false)),
                Geo::Arc { c, .. } => vals.push((get(c), 2.0, false)),
            }
        }
        let groups = cluster_1d(vals, tol);
        let snap = snapper(&groups);
        let set = |p: &mut V2| {
            if axis == 0 {
                p.x = snap(p.x)
            } else {
                p.y = snap(p.y)
            }
        };
        for g in geos.iter_mut() {
            match g {
                Geo::Line(a, b) => {
                    set(a);
                    set(b);
                }
                Geo::Circle(c, _) | Geo::Arc { c, .. } => set(c),
            }
        }
    }
}

struct Dsu(Vec<usize>);
impl Dsu {
    fn find(&mut self, i: usize) -> usize {
        if self.0[i] != i {
            let r = self.find(self.0[i]);
            self.0[i] = r;
        }
        self.0[i]
    }
    fn union(&mut self, a: usize, b: usize) {
        let (ra, rb) = (self.find(a), self.find(b));
        if ra != rb {
            self.0[ra] = rb;
        }
    }
}

fn set_end(g: &mut Geo, end: usize, p: V2) {
    match g {
        Geo::Line(a, b) => {
            if end == 0 {
                *a = p
            } else {
                *b = p
            }
        }
        Geo::Arc { .. } => {
            let (s, e) = g.endpoints().unwrap();
            let m = g.arc_mid().unwrap();
            let (s, e) = if end == 0 { (p, e) } else { (s, p) };
            if let Some((c, r, a0, a1)) = arc_3p(s, m, e) {
                // Preserve the original start/end labelling.
                let (st, en) = if (V2::polar(1.0, a0) * r + c).dist(s) < 1e-6 { (a0, a1) } else { (a1, a0) };
                *g = Geo::Arc { c, r, start: st, end: en };
            }
        }
        Geo::Circle(..) => {}
    }
}

/// Weld endpoints closer than `tol`, keeping axis-aligned lines exact.
pub fn weld(geos: &mut [Geo], tol: f64) {
    let mut ends: Vec<(usize, usize, V2)> = Vec::new();
    for (i, g) in geos.iter().enumerate() {
        if let Some((a, b)) = g.endpoints() {
            ends.push((i, 0, a));
            ends.push((i, 1, b));
        }
    }
    let mut dsu = Dsu((0..ends.len()).collect());
    for i in 0..ends.len() {
        for j in i + 1..ends.len() {
            if ends[i].0 != ends[j].0 && ends[i].2.dist(ends[j].2) < tol {
                dsu.union(i, j);
            }
        }
    }
    let mut groups: std::collections::HashMap<usize, Vec<usize>> = Default::default();
    for i in 0..ends.len() {
        let r = dsu.find(i);
        groups.entry(r).or_default().push(i);
    }
    for members in groups.values() {
        if members.len() < 2 {
            continue;
        }
        let avg = members.iter().fold(V2::ZERO, |a, m| a + ends[*m].2) / members.len() as f64;
        let vx: Vec<f64> = members.iter().filter(|m| is_v(&geos[ends[**m].0])).map(|m| ends[*m].2.x).collect();
        let hy: Vec<f64> = members.iter().filter(|m| is_h(&geos[ends[**m].0])).map(|m| ends[*m].2.y).collect();
        let x = if vx.is_empty() { avg.x } else { vx.iter().sum::<f64>() / vx.len() as f64 };
        let y = if hy.is_empty() { avg.y } else { hy.iter().sum::<f64>() / hy.len() as f64 };
        let target = V2::new(x, y);
        for m in members {
            let (gi, end, _) = ends[*m];
            set_end(&mut geos[gi], end, target);
        }
    }
}

fn endpoint_degree(geos: &[Geo], p: V2, skip: usize) -> usize {
    geos.iter()
        .enumerate()
        .filter(|(i, _)| *i != skip)
        .filter(|(_, g)| match g.endpoints() {
            Some((a, b)) => a.dist(p) < 0.75 || b.dist(p) < 0.75,
            None => false,
        })
        .count()
}

/// Snap dangling line ends onto nearby lines/circles (T-junctions, overshoots).
pub fn t_junctions(geos: &mut [Geo], tol: f64) {
    for i in 0..geos.len() {
        let Geo::Line(a, b) = geos[i] else { continue };
        for end in 0..2 {
            let p = if end == 0 { a } else { b };
            if endpoint_degree(geos, p, i) > 0 {
                continue;
            }
            let mut best: Option<(f64, V2)> = None;
            for (j, g) in geos.iter().enumerate() {
                if j == i || g.distance(p) > tol {
                    continue;
                }
                let cand = match *g {
                    Geo::Line(c, d) => line_line(a, b, c, d).and_then(|(q, _, u)| {
                        let margin = tol / c.dist(d).max(1e-9);
                        (u > -margin && u < 1.0 + margin).then_some(q)
                    }),
                    Geo::Circle(c, r) | Geo::Arc { c, r, .. } => {
                        line_circle(a, b, c, r).into_iter().map(|(q, _)| q).min_by(|x, y| x.dist(p).total_cmp(&y.dist(p)))
                    }
                };
                if let Some(q) = cand {
                    let d = q.dist(p);
                    if d < tol * 1.5 && best.is_none_or(|bb| d < bb.0) {
                        best = Some((d, q));
                    }
                }
            }
            if let Some((_, q)) = best {
                set_end(&mut geos[i], end, q);
            }
        }
    }
}

/// Remove noise: degenerate lines and short strokes with a free end.
pub fn remove_tiny(geos: &mut Vec<Geo>, min_len: f64) {
    loop {
        let before = geos.len();
        let snapshot = geos.clone();
        let mut keep = Vec::with_capacity(geos.len());
        for (i, g) in snapshot.iter().enumerate() {
            let ok = match *g {
                Geo::Line(a, b) => {
                    let l = a.dist(b);
                    if l < 1.0 {
                        false
                    } else if l < min_len {
                        endpoint_degree(&snapshot, a, i) > 0 && endpoint_degree(&snapshot, b, i) > 0
                    } else {
                        true
                    }
                }
                Geo::Arc { r, .. } => r > 1.0 && g.length() > min_len * 0.5,
                Geo::Circle(_, r) => r > 1.0,
            };
            keep.push(ok);
        }
        let mut k = keep.into_iter();
        geos.retain(|_| k.next().unwrap());
        if geos.len() == before {
            break;
        }
    }
}

/// Arcs that share a centre and radius and together cover most of a circle
/// (e.g. a circle cut by centre lines) become one circle; duplicate circles merge.
pub fn unify_circles(geos: &mut Vec<Geo>, tol: f64) {
    let mut used = vec![false; geos.len()];
    let mut add = Vec::new();
    for i in 0..geos.len() {
        let Geo::Arc { c, r, .. } = geos[i] else { continue };
        if used[i] {
            continue;
        }
        let group: Vec<usize> = (0..geos.len())
            .filter(|j| !used[*j])
            .filter(|j| matches!(geos[*j], Geo::Arc { c: c2, r: r2, .. } if c2.dist(c) < tol && (r2 - r).abs() < tol))
            .collect();
        let sweep: f64 = group.iter().map(|j| if let Geo::Arc { start, end, .. } = geos[*j] { arc_sweep(start, end) } else { 0.0 }).sum();
        if group.len() >= 2 && sweep > 290f64.to_radians() {
            let n = group.len() as f64;
            let (mut cc, mut rr) = (V2::ZERO, 0.0);
            for j in &group {
                if let Geo::Arc { c, r, .. } = geos[*j] {
                    cc += c;
                    rr += r;
                }
                used[*j] = true;
            }
            add.push(Geo::Circle(cc / n, rr / n));
        }
    }
    let mut k = used.into_iter();
    geos.retain(|_| !k.next().unwrap());
    geos.extend(add);
    // Merge near-duplicate circles (double-traced pencil circles).
    let mut i = 0;
    while i < geos.len() {
        if let Geo::Circle(c, r) = geos[i] {
            let mut j = i + 1;
            while j < geos.len() {
                if let Geo::Circle(c2, r2) = geos[j] {
                    if c.dist(c2) < tol && (r - r2).abs() < tol * 0.6 {
                        geos[i] = Geo::Circle(c.mid(c2), (r + r2) / 2.0);
                        geos.remove(j);
                        continue;
                    }
                }
                j += 1;
            }
        }
        i += 1;
    }
}

pub fn run(geos: &mut Vec<Geo>, t: &Tol) {
    snap_angles(geos, t.snap_deg);
    unify_circles(geos, t.align);
    merge_collinear(geos, t.tight, t.align);
    align_axes(geos, t.align);
    weld(geos, t.align);
    merge_collinear(geos, t.tight, t.align * 0.5);
    t_junctions(geos, t.align * 1.5);
    remove_tiny(geos, t.min_len);
    weld(geos, t.align * 0.5);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wobbly_rectangle_becomes_exact() {
        let mut g = vec![
            Geo::Line(V2::new(1.0, 2.0), V2::new(99.0, -1.5)),
            Geo::Line(V2::new(101.0, 0.5), V2::new(98.5, 51.0)),
            Geo::Line(V2::new(100.5, 49.0), V2::new(-1.0, 50.5)),
            Geo::Line(V2::new(0.5, 51.5), V2::new(-0.5, 1.0)),
        ];
        run(&mut g, &Tol { align: 5.0, tight: 3.0, min_len: 6.0, snap_deg: 7.0 });
        assert_eq!(g.len(), 4);
        for l in &g {
            assert!(is_h(l) || is_v(l), "{l:?}");
        }
        // All corners shared by exactly two lines.
        let ends: Vec<V2> = g
            .iter()
            .flat_map(|l| {
                let (a, b) = l.endpoints().unwrap();
                [a, b]
            })
            .collect();
        for e in &ends {
            assert_eq!(ends.iter().filter(|o| o.dist(*e) < 1e-9).count(), 2);
        }
    }
}
