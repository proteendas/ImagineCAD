// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Proteen Das

//! Geometric refinement after the per-axis solve: slanted lengths, angles,
//! arc radii and fillet tangency are not axis-separable, so they are enforced
//! here by a damped Gauss–Newton (Levenberg–Marquardt) solve over the
//! drawing's vertices. Horizontal/vertical dimensions and horizontal/vertical
//! lines are held as constraints too, and every vertex is weakly tied to its
//! position from the per-axis solve, which keeps the rest of the drawing put.

use icad_core::geom::*;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Con {
    /// |b − a| = value.
    Dist {
        a: usize,
        b: usize,
        value: f64,
    },
    /// b.x − a.x = value (signed).
    Dx {
        a: usize,
        b: usize,
        value: f64,
    },
    /// b.y − a.y = value (signed).
    Dy {
        a: usize,
        b: usize,
        value: f64,
    },
    Horizontal {
        a: usize,
        b: usize,
    },
    Vertical {
        a: usize,
        b: usize,
    },
    /// |p − c| = value.
    Radius {
        c: usize,
        p: usize,
        value: f64,
    },
    /// The radius c→p is perpendicular to the line p→q (tangency at p).
    Tangent {
        c: usize,
        p: usize,
        q: usize,
    },
    /// Angle from (a − v) to (b − v), counter-clockwise, in radians.
    Angle {
        v: usize,
        a: usize,
        b: usize,
        value: f64,
    },
    /// |p − c| = |q − c| (both ends of an arc).
    Equal {
        c: usize,
        p: usize,
        q: usize,
    },
}

impl Con {
    fn vars(&self) -> Vec<usize> {
        match *self {
            Con::Dist { a, b, .. } | Con::Dx { a, b, .. } | Con::Dy { a, b, .. } | Con::Horizontal { a, b } | Con::Vertical { a, b } => {
                vec![a, b]
            }
            Con::Radius { c, p, .. } => vec![c, p],
            Con::Tangent { c, p, q } => vec![c, p, q],
            Con::Angle { v, a, b, .. } => vec![v, a, b],
            Con::Equal { c, p, q } => vec![c, p, q],
        }
    }

    /// Residual, scaled to a length (so all constraints weigh alike).
    fn residual(&self, x: &[V2], scale: f64) -> f64 {
        match *self {
            Con::Dist { a, b, value } => x[a].dist(x[b]) - value,
            Con::Dx { a, b, value } => x[b].x - x[a].x - value,
            Con::Dy { a, b, value } => x[b].y - x[a].y - value,
            Con::Horizontal { a, b } => x[b].y - x[a].y,
            Con::Vertical { a, b } => x[b].x - x[a].x,
            Con::Radius { c, p, value } => x[c].dist(x[p]) - value,
            Con::Tangent { c, p, q } => {
                let (r, t) = (x[p] - x[c], (x[q] - x[p]).norm());
                r.dot(t)
            }
            Con::Angle { v, a, b, value } => {
                let (u, w) = (x[a] - x[v], x[b] - x[v]);
                let ang = norm_angle(w.angle() - u.angle());
                let mut d = ang - value;
                if d > std::f64::consts::PI {
                    d -= std::f64::consts::TAU;
                }
                if d < -std::f64::consts::PI {
                    d += std::f64::consts::TAU;
                }
                d * scale
            }
            Con::Equal { c, p, q } => x[c].dist(x[p]) - x[c].dist(x[q]),
        }
    }
}

/// Above this many vertices the dense solve is skipped.
pub const MAX_POINTS: usize = 160;

pub struct Refined {
    pub pts: Vec<V2>,
    /// Constraints still off after the solve (index into the input).
    pub unmet: Vec<usize>,
}

/// Move `pts` to satisfy `cons`; `scale` is a typical length of the drawing.
pub fn refine(pts: &[V2], cons: &[Con], scale: f64) -> Refined {
    let n = pts.len();
    let mut x = pts.to_vec();
    if cons.is_empty() || n == 0 || n > MAX_POINTS {
        return Refined { pts: x, unmet: vec![] };
    }
    let w_hard = 1.0;
    let w_soft = 1e-3;
    let tol = (scale * 1e-10).max(1e-12);
    let mut lambda = 1e-3;
    // The soft tie is re-anchored to the current solution now and then
    // (proximal steps), so the hard constraints end up met exactly while the
    // drawing still moves as little as it can.
    let mut anchor = pts.to_vec();
    let cost = |x: &[V2], anchor: &[V2]| -> f64 {
        let hard: f64 = cons.iter().map(|c| c.residual(x, scale).powi(2)).sum::<f64>() * w_hard;
        let soft: f64 = x.iter().zip(anchor).map(|(p, q)| p.dist(*q).powi(2)).sum::<f64>() * w_soft;
        hard + soft
    };
    let mut current = cost(&x, &anchor);
    for iter in 0..120 {
        if iter % 20 == 19 {
            anchor = x.clone();
            current = cost(&x, &anchor);
        }
        // Normal equations J^T J δ = −J^T r over 2n unknowns.
        let m = 2 * n;
        let mut jtj = vec![vec![0.0; m]; m];
        let mut jtr = vec![0.0; m];
        let h = (scale * 1e-6).max(1e-9);
        for c in cons {
            let r = c.residual(&x, scale);
            let vars = c.vars();
            let mut grad: Vec<(usize, f64)> = vec![];
            for &vi in &vars {
                for axis in 0..2 {
                    let mut xp = x.clone();
                    let mut xm = x.clone();
                    if axis == 0 {
                        xp[vi].x += h;
                        xm[vi].x -= h;
                    } else {
                        xp[vi].y += h;
                        xm[vi].y -= h;
                    }
                    let g = (c.residual(&xp, scale) - c.residual(&xm, scale)) / (2.0 * h);
                    if g != 0.0 {
                        grad.push((2 * vi + axis, g));
                    }
                }
            }
            for &(i, gi) in &grad {
                jtr[i] += w_hard * gi * r;
                for &(j, gj) in &grad {
                    jtj[i][j] += w_hard * gi * gj;
                }
            }
        }
        for (i, (p, q)) in x.iter().zip(&anchor).enumerate() {
            jtj[2 * i][2 * i] += w_soft;
            jtj[2 * i + 1][2 * i + 1] += w_soft;
            jtr[2 * i] += w_soft * (p.x - q.x);
            jtr[2 * i + 1] += w_soft * (p.y - q.y);
        }
        let mut improved = false;
        for _ in 0..8 {
            let mut a = jtj.clone();
            for (i, row) in a.iter_mut().enumerate() {
                row[i] += lambda * (1.0 + row[i]);
            }
            let Some(d) = crate::solve::solve_dense(a, jtr.iter().map(|v| -v).collect()) else { break };
            let trial: Vec<V2> = x.iter().enumerate().map(|(i, p)| V2::new(p.x + d[2 * i], p.y + d[2 * i + 1])).collect();
            let c = cost(&trial, &anchor);
            if c < current {
                x = trial;
                current = c;
                lambda = (lambda * 0.3).max(1e-12);
                improved = true;
                break;
            }
            lambda *= 10.0;
        }
        let worst = cons.iter().map(|c| c.residual(&x, scale).abs()).fold(0.0, f64::max);
        if worst < tol {
            break;
        }
        if !improved {
            if anchor == x {
                break;
            }
            anchor = x.clone();
            current = cost(&x, &anchor);
            lambda = 1e-3;
        }
    }
    let unmet = cons.iter().enumerate().filter(|(_, c)| c.residual(&x, scale).abs() > (scale * 1e-5).max(1e-6)).map(|(i, _)| i).collect();
    Refined { pts: x, unmet }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slanted_length_angle_and_fillet() {
        // An L-bracket outline with a slanted edge: (0,0) (100,0) (100,30) (60,60) (0,60).
        let mut p = vec![V2::new(0.0, 0.0), V2::new(100.0, 0.0), V2::new(100.0, 30.0), V2::new(60.0, 60.0), V2::new(0.0, 60.0)];
        let cons = vec![
            Con::Horizontal { a: 0, b: 1 },
            Con::Vertical { a: 1, b: 2 },
            Con::Horizontal { a: 3, b: 4 },
            Con::Vertical { a: 4, b: 0 },
            Con::Dx { a: 0, b: 1, value: 100.0 },
            Con::Dy { a: 0, b: 4, value: 60.0 },
            // The slanted edge must be 55 long at 135°.
            Con::Dist { a: 2, b: 3, value: 55.0 },
            Con::Angle { v: 2, a: 3, b: 1, value: 135f64.to_radians() },
        ];
        let r = refine(&p, &cons, 100.0);
        assert!(r.unmet.is_empty(), "{:?}", r.unmet);
        let q = &r.pts;
        assert!((q[2].dist(q[3]) - 55.0).abs() < 1e-6);
        assert!((q[1].x - q[0].x - 100.0).abs() < 1e-6 && (q[1].y - q[0].y).abs() < 1e-6);
        let ang = norm_angle((q[1] - q[2]).angle() - (q[3] - q[2]).angle());
        assert!((ang.to_degrees() - 135.0).abs() < 1e-5, "{}", ang.to_degrees());
        // A fillet: centre 2, arc ends 0 and 1 tangent to lines 0→3 and 1→4.
        p = vec![V2::new(8.0, 0.0), V2::new(0.0, 8.5), V2::new(8.3, 8.2), V2::new(50.0, 0.0), V2::new(0.0, 50.0)];
        let cons = vec![
            Con::Horizontal { a: 0, b: 3 },
            Con::Vertical { a: 1, b: 4 },
            Con::Radius { c: 2, p: 0, value: 10.0 },
            Con::Radius { c: 2, p: 1, value: 10.0 },
            Con::Tangent { c: 2, p: 0, q: 3 },
            Con::Tangent { c: 2, p: 1, q: 4 },
        ];
        let r = refine(&p, &cons, 50.0);
        assert!(r.unmet.is_empty());
        let q = &r.pts;
        assert!((q[2].dist(q[0]) - 10.0).abs() < 1e-6 && (q[2].x - q[0].x).abs() < 1e-5, "{q:?}");
    }
}
