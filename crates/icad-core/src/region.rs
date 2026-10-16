// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Proteen Das

//! Regions: closed 2D areas (outer boundaries with holes) with boolean
//! operations (union, subtract, intersect) and mass properties.

use crate::geom::*;
use i_overlay::core::fill_rule::FillRule;
use i_overlay::core::overlay_rule::OverlayRule;
use i_overlay::float::single::SingleFloatOverlay;
use serde::{Deserialize, Serialize};

/// A region: outer boundaries counter-clockwise, holes clockwise.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Region {
    pub loops: Vec<Vec<V2>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BoolOp {
    Union,
    Subtract,
    Intersect,
}

fn to_paths(loops: &[Vec<V2>]) -> Vec<Vec<[f64; 2]>> {
    loops.iter().filter(|l| l.len() >= 3).map(|l| l.iter().map(|p| [p.x, p.y]).collect()).collect()
}

fn from_shapes(shapes: Vec<Vec<Vec<[f64; 2]>>>) -> Vec<Region> {
    shapes
        .into_iter()
        .map(|s| Region { loops: s.into_iter().map(|c| c.into_iter().map(|p| V2::new(p[0], p[1])).collect()).collect() })
        .filter(|r: &Region| r.area() > 0.0)
        .collect()
}

impl Region {
    /// Regions from loops filled with the even–odd rule (islands become holes);
    /// separate outer boundaries give separate regions.
    pub fn from_loops(loops: &[Vec<V2>]) -> Vec<Region> {
        let paths = to_paths(loops);
        if paths.is_empty() {
            return vec![];
        }
        from_shapes(paths.overlay(&Vec::<Vec<[f64; 2]>>::new(), OverlayRule::Subject, FillRule::EvenOdd))
    }

    /// Boolean combination of two sets of regions.
    pub fn boolean(a: &[Region], b: &[Region], op: BoolOp) -> Vec<Region> {
        let pa: Vec<Vec<[f64; 2]>> = a.iter().flat_map(|r| to_paths(&r.loops)).collect();
        let pb: Vec<Vec<[f64; 2]>> = b.iter().flat_map(|r| to_paths(&r.loops)).collect();
        let rule = match op {
            BoolOp::Union => OverlayRule::Union,
            BoolOp::Subtract => OverlayRule::Difference,
            BoolOp::Intersect => OverlayRule::Intersect,
        };
        // Non-zero within each operand so overlapping regions of one set merge.
        from_shapes(pa.overlay(&pb, rule, FillRule::NonZero))
    }

    fn signed(l: &[V2]) -> f64 {
        crate::hatch::signed_area(l)
    }

    pub fn area(&self) -> f64 {
        self.loops.iter().map(|l| Self::signed(l)).sum::<f64>().abs()
    }

    pub fn perimeter(&self) -> f64 {
        self.loops.iter().map(|l| crate::prim::poly_segments(l, true).map(|(a, b)| a.dist(b)).sum::<f64>()).sum()
    }

    pub fn bbox(&self) -> BBox {
        BBox::of(self.loops.iter().flatten().copied())
    }

    /// Area, centroid and second moments about the origin (Ixx = ∫y², Iyy = ∫x², Ixy = ∫xy).
    fn integrals(&self) -> (f64, V2, f64, f64, f64) {
        let (mut a, mut cx, mut cy, mut ixx, mut iyy, mut ixy) = (0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
        for l in &self.loops {
            for (p, q) in crate::prim::poly_segments(l, true) {
                let cr = p.x * q.y - q.x * p.y;
                a += cr;
                cx += (p.x + q.x) * cr;
                cy += (p.y + q.y) * cr;
                ixx += (p.y * p.y + p.y * q.y + q.y * q.y) * cr;
                iyy += (p.x * p.x + p.x * q.x + q.x * q.x) * cr;
                ixy += (p.x * q.y + 2.0 * p.x * p.y + 2.0 * q.x * q.y + q.x * p.y) * cr;
            }
        }
        a /= 2.0;
        let s = a.signum();
        let c = if a.abs() > 0.0 { V2::new(cx / (6.0 * a), cy / (6.0 * a)) } else { V2::ZERO };
        (a.abs(), c, ixx / 12.0 * s, iyy / 12.0 * s, ixy / 24.0 * s)
    }

    pub fn mass_properties(&self) -> MassProps {
        MassProps::of(std::slice::from_ref(self))
    }
}

/// Mass properties of one or more regions (unit density, unit thickness).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MassProps {
    pub area: f64,
    pub perimeter: f64,
    pub bbox: BBox,
    pub centroid: V2,
    /// Second moments about the origin.
    pub ixx: f64,
    pub iyy: f64,
    pub ixy: f64,
    /// About the centroid.
    pub ixx_c: f64,
    pub iyy_c: f64,
    pub ixy_c: f64,
    /// Principal moments about the centroid and the angle (radians) of the first axis.
    pub i1: f64,
    pub i2: f64,
    pub principal_angle: f64,
    /// Radii of gyration about the centroidal axes.
    pub kx: f64,
    pub ky: f64,
}

impl MassProps {
    pub fn of(regions: &[Region]) -> MassProps {
        let (mut a, mut sx, mut sy, mut ixx, mut iyy, mut ixy, mut per) = (0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
        let mut bbox = BBox::empty();
        for r in regions {
            let (ra, c, x, y, xy) = r.integrals();
            a += ra;
            sx += c.x * ra;
            sy += c.y * ra;
            ixx += x;
            iyy += y;
            ixy += xy;
            per += r.perimeter();
            bbox = bbox.union(&r.bbox());
        }
        let c = if a > 0.0 { V2::new(sx / a, sy / a) } else { V2::ZERO };
        let ixx_c = ixx - a * c.y * c.y;
        let iyy_c = iyy - a * c.x * c.x;
        let ixy_c = ixy - a * c.x * c.y;
        let avg = (ixx_c + iyy_c) / 2.0;
        let rad = (((ixx_c - iyy_c) / 2.0).powi(2) + ixy_c * ixy_c).sqrt();
        let principal_angle = 0.5 * (-2.0 * ixy_c).atan2(ixx_c - iyy_c);
        MassProps {
            area: a,
            perimeter: per,
            bbox,
            centroid: c,
            ixx,
            iyy,
            ixy,
            ixx_c,
            iyy_c,
            ixy_c,
            i1: avg + rad,
            i2: avg - rad,
            principal_angle,
            kx: if a > 0.0 { (ixx_c / a).sqrt() } else { 0.0 },
            ky: if a > 0.0 { (iyy_c / a).sqrt() } else { 0.0 },
        }
    }
}

/// Join open paths (point lists) end to end into closed loops; returns the
/// loops and how many paths could not be closed.
pub fn chain_loops(mut paths: Vec<Vec<V2>>, tol: f64) -> (Vec<Vec<V2>>, usize) {
    let mut loops = vec![];
    let mut open = 0;
    while let Some(mut cur) = paths.pop() {
        loop {
            let (first, last) = (cur[0], *cur.last().unwrap());
            if cur.len() >= 3 && first.dist(last) <= tol {
                cur.pop();
                loops.push(cur);
                break;
            }
            let next = paths.iter().position(|p| p[0].dist(last) <= tol || p.last().unwrap().dist(last) <= tol);
            match next {
                Some(i) => {
                    let mut p = paths.swap_remove(i);
                    if p[0].dist(last) > tol {
                        p.reverse();
                    }
                    cur.extend(p.into_iter().skip(1));
                }
                None => {
                    open += 1;
                    break;
                }
            }
        }
    }
    (loops, open)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Vec<V2> {
        vec![V2::new(x0, y0), V2::new(x1, y0), V2::new(x1, y1), V2::new(x0, y1)]
    }

    #[test]
    fn booleans_and_mass_properties() {
        let a = Region::from_loops(&[rect(0.0, 0.0, 100.0, 50.0)]);
        let b = Region::from_loops(&[rect(80.0, 10.0, 120.0, 40.0)]);
        assert_eq!(a.len(), 1);
        assert!((Region::boolean(&a, &b, BoolOp::Union)[0].area() - (5000.0 + 600.0)).abs() < 1e-6);
        assert!((Region::boolean(&a, &b, BoolOp::Intersect)[0].area() - 600.0).abs() < 1e-6);
        let d = Region::boolean(&a, &b, BoolOp::Subtract);
        assert!((d[0].area() - 4400.0).abs() < 1e-6);
        // A plate with a hole from even–odd loops.
        let plate = Region::from_loops(&[rect(0.0, 0.0, 100.0, 50.0), rect(40.0, 15.0, 60.0, 35.0)]);
        assert_eq!(plate.len(), 1);
        assert!((plate[0].area() - (5000.0 - 400.0)).abs() < 1e-6);
        // Mass properties of a 100x50 rectangle at the origin.
        let m = a[0].mass_properties();
        assert!((m.centroid.x - 50.0).abs() < 1e-9 && (m.centroid.y - 25.0).abs() < 1e-9);
        assert!((m.ixx_c - 100.0 * 50f64.powi(3) / 12.0).abs() < 1e-6, "{m:?}");
        assert!((m.iyy_c - 50.0 * 100f64.powi(3) / 12.0).abs() < 1e-6);
        assert!(m.ixy_c.abs() < 1e-6);
        assert!((m.i1 - m.iyy_c).abs() < 1e-6 && (m.i2 - m.ixx_c).abs() < 1e-6);
        assert!((m.perimeter - 300.0).abs() < 1e-9);
        // The hole moves nothing (symmetric) but lowers the moments.
        let mp = plate[0].mass_properties();
        assert!((mp.centroid - V2::new(50.0, 25.0)).len() < 1e-9 && mp.ixx_c < m.ixx_c);
    }

    #[test]
    fn chaining() {
        let segs = vec![
            vec![V2::new(0.0, 0.0), V2::new(10.0, 0.0)],
            vec![V2::new(10.0, 10.0), V2::new(10.0, 0.0)],
            vec![V2::new(0.0, 10.0), V2::new(10.0, 10.0)],
            vec![V2::new(0.0, 10.0), V2::new(0.0, 0.0)],
            vec![V2::new(50.0, 0.0), V2::new(60.0, 0.0)],
        ];
        let (loops, open) = chain_loops(segs, 1e-9);
        assert_eq!((loops.len(), open), (1, 1));
        assert!((crate::hatch::signed_area(&loops[0]).abs() - 100.0).abs() < 1e-9);
    }
}
