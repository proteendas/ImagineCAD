// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Proteen Das

//! Associative dimensions: a dimension's points are tied to snap points of
//! the geometry they measure (or to a whole circle/arc) and follow it when it
//! is moved, stretched or resized.

use crate::document::Document;
use crate::entity::*;
use crate::geom::*;
use std::collections::HashMap;

/// Geometry a dimension may be tied to (not annotation).
fn is_geometry(s: &Shape) -> bool {
    !matches!(s, Shape::Dim(_) | Shape::Text(_) | Shape::MText(_) | Shape::Leader(_) | Shape::Hatch(_) | Shape::Image(_))
}

impl Document {
    fn assoc_tol(&self) -> f64 {
        let e = self.extents();
        if e.is_empty() {
            1e-6
        } else {
            (e.width().max(e.height()) * 1e-7).max(1e-9)
        }
    }

    /// Snap point of another entity exactly at `p`.
    fn point_ref(&self, p: V2, tol: f64) -> Option<GeomRef> {
        self.entities
            .iter()
            .filter(|e| is_geometry(&e.shape))
            .find_map(|e| e.shape.snap_points().iter().position(|(_, q)| q.dist(p) <= tol).map(|index| GeomRef { id: e.id, index }))
    }

    /// Circle or arc with centre `c` and radius `r`.
    fn curve_ref(&self, c: V2, r: f64, tol: f64) -> Option<u64> {
        self.entities.iter().find_map(|e| match e.shape {
            Shape::Circle { c: cc, r: rr } | Shape::Arc { c: cc, r: rr, .. } if cc.dist(c) <= tol && (rr - r).abs() <= tol => Some(e.id),
            _ => None,
        })
    }

    /// Tie the dimension to the geometry it measures, where its points
    /// coincide with snap points (or its circle/arc exists).
    pub fn associate_dim(&self, d: &mut Dimension) {
        let tol = self.assoc_tol();
        let mut a = DimAssoc::default();
        match d.kind {
            DimKind::Radius | DimKind::Diameter => a.curve = self.curve_ref(d.p1, d.p1.dist(d.p2), tol),
            DimKind::ArcLength => {
                let c = d.center.unwrap_or(V2::ZERO);
                a.curve = self.curve_ref(c, c.dist(d.p1), tol);
            }
            DimKind::OrdinateX | DimKind::OrdinateY => a.p1 = self.point_ref(d.p1, tol),
            DimKind::Angular => {
                a.p1 = self.point_ref(d.p1, tol);
                a.p2 = self.point_ref(d.p2, tol);
                a.center = d.center.and_then(|c| self.point_ref(c, tol));
            }
            _ => {
                a.p1 = self.point_ref(d.p1, tol);
                a.p2 = self.point_ref(d.p2, tol);
            }
        }
        d.assoc = (!a.is_empty()).then_some(a);
    }

    /// Associate every dimension that is not associative yet. Returns how many
    /// became associative.
    pub fn associate_all_dims(&mut self) -> usize {
        let mut changed = vec![];
        for (i, e) in self.entities.iter().enumerate() {
            if let Shape::Dim(d) = &e.shape {
                if d.assoc.is_none() {
                    let mut nd = d.clone();
                    self.associate_dim(&mut nd);
                    if nd.assoc.is_some() {
                        changed.push((i, nd));
                    }
                }
            }
        }
        let n = changed.len();
        for (i, d) in changed {
            self.entities[i].shape = Shape::Dim(d);
        }
        n
    }

    /// True when any dimension is associative.
    pub fn has_associative(&self) -> bool {
        self.entities.iter().any(|e| matches!(&e.shape, Shape::Dim(d) if d.assoc.is_some()))
    }

    /// Move associative dimensions with their geometry. Returns how many changed.
    pub fn update_associative(&mut self) -> usize {
        let index: HashMap<u64, usize> = self.entities.iter().enumerate().map(|(i, e)| (e.id, i)).collect();
        let shape_of = |id: u64| index.get(&id).map(|i| &self.entities[*i].shape);
        let point = |r: &Option<GeomRef>| -> Option<Option<V2>> {
            r.map(|g| shape_of(g.id).filter(|s| is_geometry(s)).and_then(|s| s.snap_points().get(g.index).map(|(_, p)| *p)))
        };
        let mut updates: Vec<(usize, Dimension)> = vec![];
        for (i, e) in self.entities.iter().enumerate() {
            let Shape::Dim(d) = &e.shape else { continue };
            let Some(a) = d.assoc else { continue };
            let mut nd = d.clone();
            let mut na = a;
            match d.kind {
                DimKind::Radius | DimKind::Diameter | DimKind::ArcLength => {
                    let curve = a.curve.and_then(shape_of).cloned();
                    match curve {
                        Some(Shape::Circle { c, r }) if d.kind != DimKind::ArcLength => {
                            let (oc, or) = (d.p1, d.p1.dist(d.p2));
                            let u = (d.p2 - oc).norm();
                            nd.p1 = c;
                            nd.p2 = c + if u == V2::ZERO { V2::new(r, 0.0) } else { u * r };
                            let ul = (d.loc - oc).norm();
                            nd.loc = c + if ul == V2::ZERO { V2::ZERO } else { ul * (r + d.loc.dist(oc) - or) };
                        }
                        Some(Shape::Arc { c, r, start, end }) => {
                            let oc = d.center.unwrap_or(d.p1);
                            let or = if d.kind == DimKind::ArcLength { oc.dist(d.p1) } else { d.p1.dist(d.p2) };
                            let ul = (d.loc - oc).norm();
                            let loc = c + if ul == V2::ZERO { V2::ZERO } else { ul * (r + d.loc.dist(oc) - or) };
                            if d.kind == DimKind::ArcLength {
                                nd.center = Some(c);
                                nd.p1 = c + V2::polar(r, start);
                                nd.p2 = c + V2::polar(r, end);
                            } else {
                                let u = (d.p2 - d.p1).norm();
                                nd.p1 = c;
                                nd.p2 = c + if u == V2::ZERO { V2::new(r, 0.0) } else { u * r };
                            }
                            nd.loc = loc;
                        }
                        _ => na.curve = None,
                    }
                }
                _ => {
                    let mut moved = vec![];
                    for (r, slot) in [(&a.p1, 0), (&a.p2, 1), (&a.center, 2)] {
                        match point(r) {
                            Some(Some(p)) => {
                                let old = match slot {
                                    0 => d.p1,
                                    1 => d.p2,
                                    _ => d.center.unwrap_or(p),
                                };
                                moved.push(p - old);
                                match slot {
                                    0 => nd.p1 = p,
                                    1 => nd.p2 = p,
                                    _ => nd.center = Some(p),
                                }
                            }
                            Some(None) => match slot {
                                0 => na.p1 = None,
                                1 => na.p2 = None,
                                _ => na.center = None,
                            },
                            None => {}
                        }
                    }
                    if d.is_ordinate() {
                        // The leader keeps its offset from the feature.
                        nd.loc = d.loc + (nd.p1 - d.p1);
                        nd.p2 = nd.p1;
                    } else if !moved.is_empty() && d.kind != DimKind::Angular {
                        let avg = moved.iter().fold(V2::ZERO, |s, v| s + *v) / moved.len() as f64;
                        nd.loc = d.loc + avg;
                    } else if d.kind == DimKind::Angular {
                        if let (Some(c0), Some(c1)) = (d.center, nd.center) {
                            nd.loc = d.loc + (c1 - c0);
                        }
                    }
                }
            }
            nd.assoc = (!na.is_empty()).then_some(na);
            if nd != *d {
                updates.push((i, nd));
            }
        }
        let n = updates.len();
        for (i, d) in updates {
            self.entities[i].shape = Shape::Dim(d);
        }
        n
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dimensions_follow_their_geometry() {
        let mut doc = Document::new();
        let line = doc.add(Shape::Line { a: V2::new(0.0, 0.0), b: V2::new(100.0, 0.0) });
        let circle = doc.add(Shape::Circle { c: V2::new(50.0, 50.0), r: 10.0 });
        let mut lin = Dimension::new(DimKind::Horizontal, V2::new(0.0, 0.0), V2::new(100.0, 0.0), V2::new(50.0, -15.0));
        doc.associate_dim(&mut lin);
        assert!(lin.assoc.unwrap().p1.is_some() && lin.assoc.unwrap().p2.is_some());
        let lin_id = doc.add(Shape::Dim(lin));
        let mut dia = Dimension::new(DimKind::Diameter, V2::new(50.0, 50.0), V2::new(60.0, 50.0), V2::new(70.0, 70.0));
        doc.associate_dim(&mut dia);
        assert_eq!(dia.assoc.unwrap().curve, Some(circle));
        let dia_id = doc.add(Shape::Dim(dia));

        // Stretch the line's end and grow the circle.
        doc.get_mut(line).unwrap().shape.set_grip(1, V2::new(120.0, 0.0));
        doc.get_mut(circle).unwrap().shape = Shape::Circle { c: V2::new(55.0, 50.0), r: 12.0 };
        assert_eq!(doc.update_associative(), 2);
        let Shape::Dim(d) = &doc.get(lin_id).unwrap().shape else { panic!() };
        assert_eq!(d.measurement(), 120.0);
        let Shape::Dim(d) = &doc.get(dia_id).unwrap().shape else { panic!() };
        assert_eq!(d.measurement(), 24.0);
        assert_eq!(d.p1, V2::new(55.0, 50.0));
        // Nothing left to do: stable.
        assert_eq!(doc.update_associative(), 0);
        // Deleting the geometry dissociates without moving the dimension.
        doc.remove(&[line]);
        doc.update_associative();
        let Shape::Dim(d) = &doc.get(lin_id).unwrap().shape else { panic!() };
        assert!(d.assoc.is_none() && d.measurement() == 120.0);
    }

    #[test]
    fn ordinate_and_arc_length() {
        let o = Dimension::ordinate(V2::ZERO, V2::new(40.0, 25.0), V2::new(40.0, 60.0));
        assert_eq!(o.kind, DimKind::OrdinateX);
        assert_eq!(o.measurement(), 40.0);
        let o = Dimension::ordinate(V2::new(10.0, 5.0), V2::new(40.0, 25.0), V2::new(80.0, 27.0));
        assert_eq!((o.kind, o.measurement()), (DimKind::OrdinateY, 20.0));
        let a = Dimension::arc_length(V2::ZERO, V2::new(10.0, 0.0), V2::new(0.0, 10.0), V2::new(12.0, 12.0));
        assert!((a.measurement() - 10.0 * std::f64::consts::FRAC_PI_2).abs() < 1e-9);
        let ds = crate::style::DimStyle::default();
        assert!(!a.prims(&ds, 1.0).is_empty() && !o.prims(&ds, 1.0).is_empty());
    }
}
