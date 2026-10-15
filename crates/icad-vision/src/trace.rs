// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Proteen Das

//! Skeleton graph tracing: turns a 1-px skeleton into pixel chains between
//! endpoints/junctions, plus closed loops.

use crate::raster::Mask;
use icad_core::geom::V2;

pub struct Path {
    /// Points in y-up pixel coordinates (pixel centres).
    pub pts: Vec<V2>,
    pub closed: bool,
    /// Connected-component label of the source ink (for text filtering).
    pub comp: u32,
}

const OFFS: [(isize, isize); 8] = [(1, 0), (0, -1), (-1, 0), (0, 1), (1, -1), (-1, -1), (-1, 1), (1, 1)];

pub fn trace(sk: &Mask, comp_labels: &[u32]) -> Vec<Path> {
    let (w, h) = (sk.w, sk.h);
    let idx = |x: isize, y: isize| y as usize * w + x as usize;
    let neighbours = |i: usize| -> Vec<usize> {
        let (x, y) = ((i % w) as isize, (i / w) as isize);
        OFFS.iter().filter(|(dx, dy)| sk.get(x + dx, y + dy) != 0).map(|(dx, dy)| idx(x + dx, y + dy)).collect()
    };
    let ink: Vec<usize> = (0..w * h).filter(|i| sk.px[*i] != 0).collect();
    let mut deg = vec![0u8; w * h];
    for &i in &ink {
        deg[i] = neighbours(i).len() as u8;
    }
    let is_node = |i: usize| deg[i] != 2;
    let mut visited = vec![false; w * h];
    let to_pt = |i: usize| V2::new((i % w) as f64 + 0.5, h as f64 - ((i / w) as f64 + 0.5));
    let mut out = Vec::new();

    let walk = |start: usize, first: usize, visited: &mut Vec<bool>| -> Vec<usize> {
        let mut path = vec![start, first];
        visited[first] = true;
        let (mut prev, mut cur) = (start, first);
        loop {
            if is_node(cur) {
                break;
            }
            let nb = neighbours(cur);
            let next = nb
                .iter()
                .copied()
                .find(|n| *n != prev && is_node(*n) && *n != start)
                .or_else(|| nb.iter().copied().find(|n| *n != prev && !visited[*n] && !is_node(*n)))
                .or_else(|| nb.iter().copied().find(|n| *n != prev && *n == start));
            match next {
                Some(n) => {
                    path.push(n);
                    if is_node(n) || n == start {
                        break;
                    }
                    visited[n] = true;
                    prev = cur;
                    cur = n;
                }
                None => break,
            }
        }
        path
    };

    for &n in &ink {
        if !is_node(n) || deg[n] == 0 {
            continue;
        }
        for m in neighbours(n) {
            if is_node(m) || visited[m] {
                continue;
            }
            let p = walk(n, m, &mut visited);
            if p.len() >= 2 {
                let closed = p.first() == p.last();
                out.push(Path { pts: p.iter().map(|i| to_pt(*i)).collect(), closed, comp: comp_labels[n] });
            }
        }
    }
    // Pure loops (no endpoints or junctions).
    for &s in &ink {
        if visited[s] || is_node(s) {
            continue;
        }
        visited[s] = true;
        let nb = neighbours(s);
        if nb.is_empty() {
            continue;
        }
        let mut p = walk(s, nb[0], &mut visited);
        if p.last() != Some(&s) {
            p.push(s);
        }
        out.push(Path { pts: p.iter().map(|i| to_pt(*i)).collect(), closed: true, comp: comp_labels[s] });
    }
    out
}
