// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Proteen Das

//! Hatching: pattern definitions, pattern-line clipping, exact solid fills and
//! automatic boundary detection ("pick internal point").

use crate::geom::*;
use std::collections::HashMap;

/// One family of parallel lines: angle (deg), spacing and offset between
/// consecutive lines (paper mm), and an optional dash pattern.
#[derive(Clone, Copy, Debug)]
pub struct Family {
    pub angle: f64,
    pub spacing: f64,
    pub shift: f64,
}

pub const PATTERNS: &[(&str, &str)] = &[
    ("SOLID", "Solid fill"),
    ("ANSI31", "ANSI iron, brick, stone masonry (45° lines)"),
    ("ANSI32", "ANSI steel (double 45° lines)"),
    ("ANSI33", "ANSI bronze, brass, copper"),
    ("ANSI37", "ANSI lead, zinc, magnesium (crosshatch)"),
    ("ANSI38", "ANSI aluminium"),
    ("NET", "Horizontal / vertical grid"),
    ("LINE", "Parallel horizontal lines"),
    ("ANGLE", "Angle steel"),
];

pub fn pattern_families(name: &str) -> Vec<Family> {
    let f = |angle: f64, spacing: f64, shift: f64| Family { angle, spacing, shift };
    match name.to_ascii_uppercase().as_str() {
        "ANSI31" => vec![f(45.0, 3.175, 0.0)],
        "ANSI32" => vec![f(45.0, 9.525, 0.0), f(45.0, 9.525, 3.175)],
        "ANSI33" => vec![f(45.0, 6.35, 0.0), f(45.0, 6.35, 3.175)],
        "ANSI37" => vec![f(45.0, 3.175, 0.0), f(135.0, 3.175, 0.0)],
        "ANSI38" => vec![f(45.0, 3.175, 0.0), f(135.0, 6.35, 0.0)],
        "NET" => vec![f(0.0, 3.175, 0.0), f(90.0, 3.175, 0.0)],
        "LINE" => vec![f(0.0, 3.175, 0.0)],
        "ANGLE" => vec![f(0.0, 6.985, 0.0), f(90.0, 6.985, 0.0)],
        _ => vec![f(45.0, 3.175, 0.0)],
    }
}

const MAX_LINES: usize = 20_000;

/// Clip a family of parallel lines against the loops (even–odd rule).
pub fn pattern_segments(loops: &[Vec<V2>], pattern: &str, scale: f64, angle: f64) -> Vec<(V2, V2)> {
    let mut out = vec![];
    let all: Vec<V2> = loops.iter().flatten().copied().collect();
    if all.len() < 3 {
        return out;
    }
    for fam in pattern_families(pattern) {
        let a = rad(fam.angle) + angle;
        let d = V2::polar(1.0, a);
        let n = d.perp();
        let mut sp = fam.spacing * scale.max(1e-9);
        let (lo, hi) = all.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), p| (lo.min(p.dot(n)), hi.max(p.dot(n))));
        // Keep the line count bounded for tiny scales.
        while (hi - lo) / sp > MAX_LINES as f64 {
            sp *= 2.0;
        }
        let shift = fam.shift * scale;
        let k0 = ((lo - shift) / sp).ceil() as i64;
        let k1 = ((hi - shift) / sp).floor() as i64;
        for k in k0..=k1 {
            let o = k as f64 * sp + shift;
            let mut ts: Vec<f64> = vec![];
            for lp in loops {
                let m = lp.len();
                for i in 0..m {
                    let (p, q) = (lp[i], lp[(i + 1) % m]);
                    let (np, nq) = (p.dot(n) - o, q.dot(n) - o);
                    if (np > 0.0) != (nq > 0.0) {
                        let t = np / (np - nq);
                        ts.push((p + (q - p) * t).dot(d));
                    }
                }
            }
            ts.sort_by(f64::total_cmp);
            for pair in ts.as_chunks::<2>().0 {
                if pair[1] - pair[0] > 1e-9 {
                    out.push((n * o + d * pair[0], n * o + d * pair[1]));
                }
            }
        }
    }
    out
}

/// Exact decomposition of the (even–odd) filled region into convex trapezoids.
pub fn solid_trapezoids(loops: &[Vec<V2>]) -> Vec<[V2; 4]> {
    let mut edges: Vec<(V2, V2)> = vec![];
    let mut ys: Vec<f64> = vec![];
    for lp in loops {
        let m = lp.len();
        if m < 3 {
            continue;
        }
        for i in 0..m {
            let (p, q) = (lp[i], lp[(i + 1) % m]);
            ys.push(p.y);
            if (p.y - q.y).abs() > 1e-12 {
                edges.push((p, q));
            }
        }
    }
    ys.sort_by(f64::total_cmp);
    ys.dedup_by(|a, b| (*a - *b).abs() < 1e-12);
    let x_at = |e: &(V2, V2), y: f64| e.0.x + (e.1.x - e.0.x) * (y - e.0.y) / (e.1.y - e.0.y);
    let mut out = vec![];
    for w in ys.windows(2) {
        let (y0, y1) = (w[0], w[1]);
        let ym = (y0 + y1) / 2.0;
        let mut xs: Vec<(f64, usize)> = edges
            .iter()
            .enumerate()
            .filter(|(_, e)| e.0.y.min(e.1.y) <= ym && e.0.y.max(e.1.y) >= ym)
            .map(|(i, e)| (x_at(e, ym), i))
            .collect();
        xs.sort_by(|a, b| a.0.total_cmp(&b.0));
        for pair in xs.as_chunks::<2>().0 {
            let (l, r) = (&edges[pair[0].1], &edges[pair[1].1]);
            out.push([V2::new(x_at(l, y0), y0), V2::new(x_at(r, y0), y0), V2::new(x_at(r, y1), y1), V2::new(x_at(l, y1), y1)]);
        }
    }
    out
}

/// Area of the even–odd region of `loops`.
pub fn region_area(loops: &[Vec<V2>]) -> f64 {
    solid_trapezoids(loops).iter().map(|t| ((t[1].x - t[0].x) + (t[2].x - t[3].x)) * 0.5 * (t[3].y - t[0].y)).sum()
}

pub fn signed_area(pts: &[V2]) -> f64 {
    let n = pts.len();
    (0..n).map(|i| pts[i].cross(pts[(i + 1) % n])).sum::<f64>() / 2.0
}

fn point_in(p: V2, pts: &[V2]) -> bool {
    crate::prim::point_in_poly(p, pts)
}

/// Find the closed region around `p` formed by `segments` (from all visible
/// geometry, curves tessellated). Returns the boundary loop followed by island
/// loops inside it, or `None` if `p` is not enclosed.
pub fn find_boundary(segments: &[(V2, V2)], p: V2) -> Option<Vec<Vec<V2>>> {
    if segments.is_empty() {
        return None;
    }
    let ext = segments.iter().fold(BBox::empty(), |b, s| b.union(&BBox::from_pts(s.0, s.1)));
    let eps = (ext.width().max(ext.height()) * 1e-9).max(1e-9);

    // 1. Split every segment at all intersections.
    let n = segments.len();
    let boxes: Vec<BBox> = segments.iter().map(|s| BBox::from_pts(s.0, s.1).expand(eps)).collect();
    let mut params: Vec<Vec<f64>> = vec![vec![0.0, 1.0]; n];
    // Sweep by x to avoid the full O(n²) pair test.
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by(|a, b| boxes[*a].min.x.total_cmp(&boxes[*b].min.x));
    for (oi, &i) in order.iter().enumerate() {
        for &j in &order[oi + 1..] {
            if boxes[j].min.x > boxes[i].max.x {
                break;
            }
            if !boxes[i].intersects(&boxes[j]) {
                continue;
            }
            let (a1, a2) = segments[i];
            let (b1, b2) = segments[j];
            if let Some((_, t, u)) = line_line(a1, a2, b1, b2) {
                let te = 1e-9;
                if (-te..=1.0 + te).contains(&t) && (-te..=1.0 + te).contains(&u) {
                    params[i].push(t.clamp(0.0, 1.0));
                    params[j].push(u.clamp(0.0, 1.0));
                }
            } else {
                // Collinear overlap: split at each other's endpoints.
                for (s, o) in [(i, j), (j, i)] {
                    let (a, b) = segments[s];
                    for q in [segments[o].0, segments[o].1] {
                        if dist_point_segment(q, a, b) < eps * 10.0 {
                            params[s].push(project_t(q, a, b).clamp(0.0, 1.0));
                        }
                    }
                }
            }
        }
    }

    // 2. Nodes (snapped on a grid) and undirected edges.
    let cell = eps * 1000.0;
    let mut node_of: HashMap<(i64, i64), usize> = HashMap::new();
    let mut nodes: Vec<V2> = vec![];
    let mut node = |q: V2, nodes: &mut Vec<V2>| -> usize {
        let key = ((q.x / cell).round() as i64, (q.y / cell).round() as i64);
        *node_of.entry(key).or_insert_with(|| {
            nodes.push(q);
            nodes.len() - 1
        })
    };
    let mut edges: std::collections::BTreeSet<(usize, usize)> = Default::default();
    for (i, ps) in params.iter_mut().enumerate() {
        ps.sort_by(f64::total_cmp);
        ps.dedup_by(|a, b| (*a - *b).abs() < 1e-12);
        let (a, b) = segments[i];
        for w in ps.windows(2) {
            let u = node(a.lerp(b, w[0]), &mut nodes);
            let v = node(a.lerp(b, w[1]), &mut nodes);
            if u != v {
                edges.insert((u.min(v), u.max(v)));
            }
        }
    }

    // 3. Prune dangling edges.
    let mut adj: Vec<Vec<usize>> = vec![vec![]; nodes.len()];
    for &(u, v) in &edges {
        adj[u].push(v);
        adj[v].push(u);
    }
    let mut changed = true;
    while changed {
        changed = false;
        for u in 0..nodes.len() {
            if adj[u].len() == 1 {
                let v = adj[u][0];
                adj[u].clear();
                adj[v].retain(|x| *x != u);
                changed = true;
            }
        }
    }
    // Sort neighbours counter-clockwise.
    for (u, nb) in adj.iter_mut().enumerate() {
        let c = nodes[u];
        nb.sort_by(|a, b| (nodes[*a] - c).angle().total_cmp(&(nodes[*b] - c).angle()));
    }

    // 4. Trace faces: for half-edge u→v, continue with the neighbour of v that
    //    precedes u in CCW order (the face lies to the left).
    let mut used: std::collections::HashSet<(usize, usize)> = Default::default();
    let mut faces: Vec<Vec<usize>> = vec![];
    for u in 0..nodes.len() {
        for &v in &adj[u] {
            if used.contains(&(u, v)) {
                continue;
            }
            let mut face = vec![];
            let (mut a, mut b) = (u, v);
            let mut guard = 0;
            while used.insert((a, b)) {
                face.push(a);
                let nb = &adj[b];
                let i = nb.iter().position(|x| *x == a).unwrap_or(0);
                let next = nb[(i + nb.len() - 1) % nb.len()];
                a = b;
                b = next;
                guard += 1;
                if guard > 1_000_000 {
                    break;
                }
            }
            if face.len() >= 3 {
                faces.push(face);
            }
        }
    }
    let loops: Vec<Vec<V2>> = faces.iter().map(|f| f.iter().map(|i| nodes[*i]).collect()).collect();
    let areas: Vec<f64> = loops.iter().map(|l| signed_area(l)).collect();

    // 5. The smallest bounded (positive) face containing p.
    let outer = (0..loops.len()).filter(|i| areas[*i] > 0.0 && point_in(p, &loops[*i])).min_by(|a, b| areas[*a].total_cmp(&areas[*b]))?;
    let mut result = vec![loops[outer].clone()];
    // 6. Islands: outer boundaries (negative faces) of other components inside it.
    let comp = connected_components(&adj);
    let outer_comp = comp[faces[outer][0]];
    for (i, l) in loops.iter().enumerate() {
        if areas[i] < 0.0 && comp[faces[i][0]] != outer_comp && l.iter().all(|q| point_in(*q, &loops[outer])) && !point_in(p, l) {
            result.push(l.clone());
        }
    }
    Some(result)
}

fn connected_components(adj: &[Vec<usize>]) -> Vec<usize> {
    let mut comp = vec![usize::MAX; adj.len()];
    let mut c = 0;
    for s in 0..adj.len() {
        if comp[s] != usize::MAX {
            continue;
        }
        let mut stack = vec![s];
        comp[s] = c;
        while let Some(u) = stack.pop() {
            for &v in &adj[u] {
                if comp[v] == usize::MAX {
                    comp[v] = c;
                    stack.push(v);
                }
            }
        }
        c += 1;
    }
    comp
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Vec<(V2, V2)> {
        let p = [V2::new(x0, y0), V2::new(x1, y0), V2::new(x1, y1), V2::new(x0, y1)];
        (0..4).map(|i| (p[i], p[(i + 1) % 4])).collect()
    }

    #[test]
    fn boundary_with_island_and_area() {
        let mut segs = rect(0.0, 0.0, 100.0, 50.0);
        segs.extend(rect(40.0, 10.0, 60.0, 30.0)); // island
        segs.push((V2::new(-20.0, 25.0), V2::new(0.0, 25.0))); // dangling stub
        let loops = find_boundary(&segs, V2::new(10.0, 10.0)).unwrap();
        assert_eq!(loops.len(), 2);
        assert!((region_area(&loops) - (5000.0 - 400.0)).abs() < 1e-6);
        // Clicking inside the island hatches the island only.
        let inner = find_boundary(&segs, V2::new(50.0, 20.0)).unwrap();
        assert!((region_area(&inner) - 400.0).abs() < 1e-6);
        assert!(find_boundary(&segs, V2::new(200.0, 0.0)).is_none());
    }

    #[test]
    fn crossing_lines_split_faces() {
        let mut segs = rect(0.0, 0.0, 100.0, 100.0);
        segs.push((V2::new(50.0, -10.0), V2::new(50.0, 110.0)));
        let loops = find_boundary(&segs, V2::new(75.0, 50.0)).unwrap();
        assert!((region_area(&loops) - 5000.0).abs() < 1e-6);
    }

    #[test]
    fn pattern_lines_stay_inside() {
        let sq = vec![vec![V2::new(0.0, 0.0), V2::new(10.0, 0.0), V2::new(10.0, 10.0), V2::new(0.0, 10.0)]];
        let segs = pattern_segments(&sq, "ANSI31", 1.0, 0.0);
        assert!(!segs.is_empty());
        for (a, b) in segs {
            for q in [a, b] {
                assert!(q.x > -1e-9 && q.x < 10.0 + 1e-9 && q.y > -1e-9 && q.y < 10.0 + 1e-9);
            }
        }
    }
}
