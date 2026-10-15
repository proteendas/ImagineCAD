// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Proteen Das

//! Raster stage: loading, adaptive binarisation, morphology, connected
//! components and Zhang–Suen thinning. Pure Rust, no OpenCV.

use image::{GrayImage, ImageBuffer, Luma};

/// Grayscale image (row-major, 0 = black).
#[derive(Clone)]
pub struct Gray {
    pub w: usize,
    pub h: usize,
    pub px: Vec<u8>,
}

/// Binary mask (row-major, 1 = ink).
#[derive(Clone)]
pub struct Mask {
    pub w: usize,
    pub h: usize,
    pub px: Vec<u8>,
}

impl Mask {
    pub fn get(&self, x: isize, y: isize) -> u8 {
        if x < 0 || y < 0 || x >= self.w as isize || y >= self.h as isize {
            0
        } else {
            self.px[y as usize * self.w + x as usize]
        }
    }
    pub fn count(&self) -> usize {
        self.px.iter().filter(|v| **v != 0).count()
    }
    /// Render as black-ink-on-white grayscale image.
    pub fn to_image(&self) -> GrayImage {
        ImageBuffer::from_fn(self.w as u32, self.h as u32, |x, y| {
            Luma([if self.px[y as usize * self.w + x as usize] != 0 { 0 } else { 255 }])
        })
    }
}

/// Decode an image, convert to grayscale and resize so the longer side is
/// between ~1000 px and `max_dim`.
pub fn load(bytes: &[u8], max_dim: u32) -> Result<Gray, String> {
    let img = image::load_from_memory(bytes).map_err(|e| format!("Cannot read image: {e}"))?;
    // Flatten transparency onto white.
    let rgba = img.to_rgba8();
    let (w, h) = rgba.dimensions();
    let mut g = GrayImage::new(w, h);
    for (x, y, p) in rgba.enumerate_pixels() {
        let a = p[3] as f32 / 255.0;
        let l = 0.299 * p[0] as f32 + 0.587 * p[1] as f32 + 0.114 * p[2] as f32;
        g.put_pixel(x, y, Luma([(l * a + 255.0 * (1.0 - a)).round() as u8]));
    }
    let long = w.max(h).max(1);
    let target = if long > max_dim {
        max_dim
    } else if long < 900 {
        1000.min(max_dim)
    } else {
        long
    };
    let g = if target != long {
        let k = target as f64 / long as f64;
        let nw = ((w as f64 * k).round() as u32).max(1);
        let nh = ((h as f64 * k).round() as u32).max(1);
        image::imageops::resize(&g, nw, nh, image::imageops::FilterType::Triangle)
    } else {
        g
    };
    Ok(Gray { w: g.width() as usize, h: g.height() as usize, px: g.into_raw() })
}

impl Gray {
    pub fn to_image(&self) -> GrayImage {
        ImageBuffer::from_raw(self.w as u32, self.h as u32, self.px.clone()).unwrap()
    }

    fn percentile(&self, p: f64) -> u8 {
        let mut hist = [0usize; 256];
        for v in &self.px {
            hist[*v as usize] += 1;
        }
        let target = (self.px.len() as f64 * p) as usize;
        let mut acc = 0;
        for (i, c) in hist.iter().enumerate() {
            acc += c;
            if acc >= target {
                return i as u8;
            }
        }
        255
    }

    /// Bradley–Roth adaptive threshold combined with a paper-relative global
    /// limit (so faint grid lines on squared paper are ignored).
    pub fn binarize(&self) -> Mask {
        let (w, h) = (self.w, self.h);
        let mut integral = vec![0u64; (w + 1) * (h + 1)];
        for y in 0..h {
            let mut row = 0u64;
            for x in 0..w {
                row += self.px[y * w + x] as u64;
                integral[(y + 1) * (w + 1) + x + 1] = integral[y * (w + 1) + x + 1] + row;
            }
        }
        let paper = self.percentile(0.9) as f64;
        let global = paper * 0.80;
        let s = (w.max(h) / 14).max(8);
        let t = 0.14;
        let mut px = vec![0u8; w * h];
        for y in 0..h {
            let y0 = y.saturating_sub(s / 2);
            let y1 = (y + s / 2).min(h - 1);
            for x in 0..w {
                let x0 = x.saturating_sub(s / 2);
                let x1 = (x + s / 2).min(w - 1);
                let count = ((x1 - x0 + 1) * (y1 - y0 + 1)) as f64;
                let sum = integral[(y1 + 1) * (w + 1) + x1 + 1] + integral[y0 * (w + 1) + x0]
                    - integral[y0 * (w + 1) + x1 + 1]
                    - integral[(y1 + 1) * (w + 1) + x0];
                let v = self.px[y * w + x] as f64;
                if v * count <= sum as f64 * (1.0 - t) && v < global {
                    px[y * w + x] = 1;
                }
            }
        }
        Mask { w, h, px }
    }
}

fn morph(m: &Mask, dilate: bool) -> Mask {
    let mut out = m.clone();
    for y in 0..m.h as isize {
        for x in 0..m.w as isize {
            let mut v = if dilate { 0 } else { 1 };
            'n: for dy in -1..=1 {
                for dx in -1..=1 {
                    let p = m.get(x + dx, y + dy);
                    if dilate && p != 0 {
                        v = 1;
                        break 'n;
                    }
                    if !dilate && p == 0 {
                        v = 0;
                        break 'n;
                    }
                }
            }
            out.px[y as usize * m.w + x as usize] = v;
        }
    }
    out
}

/// Morphological closing (fills pinholes in pencil strokes, bridges 1px gaps).
pub fn close(m: &Mask) -> Mask {
    morph(&morph(m, true), false)
}

#[derive(Clone, Debug)]
pub struct Component {
    pub count: usize,
    pub x0: usize,
    pub y0: usize,
    pub x1: usize,
    pub y1: usize,
}

impl Component {
    pub fn w(&self) -> usize {
        self.x1 - self.x0 + 1
    }
    pub fn h(&self) -> usize {
        self.y1 - self.y0 + 1
    }
}

/// 8-connected component labelling. Labels are 1-based (0 = background).
pub fn components(m: &Mask) -> (Vec<u32>, Vec<Component>) {
    let mut labels = vec![0u32; m.w * m.h];
    let mut comps = Vec::new();
    let mut stack = Vec::new();
    for start in 0..m.px.len() {
        if m.px[start] == 0 || labels[start] != 0 {
            continue;
        }
        let id = comps.len() as u32 + 1;
        let mut c = Component { count: 0, x0: usize::MAX, y0: usize::MAX, x1: 0, y1: 0 };
        labels[start] = id;
        stack.push(start);
        while let Some(i) = stack.pop() {
            let (x, y) = (i % m.w, i / m.w);
            c.count += 1;
            c.x0 = c.x0.min(x);
            c.x1 = c.x1.max(x);
            c.y0 = c.y0.min(y);
            c.y1 = c.y1.max(y);
            for dy in -1isize..=1 {
                for dx in -1isize..=1 {
                    let (nx, ny) = (x as isize + dx, y as isize + dy);
                    if nx < 0 || ny < 0 || nx >= m.w as isize || ny >= m.h as isize {
                        continue;
                    }
                    let j = ny as usize * m.w + nx as usize;
                    if m.px[j] != 0 && labels[j] == 0 {
                        labels[j] = id;
                        stack.push(j);
                    }
                }
            }
        }
        comps.push(c);
    }
    (labels, comps)
}

/// Zhang–Suen thinning followed by removal of staircase corner pixels, giving
/// a one-pixel-wide 8-connected skeleton.
pub fn thin(m: &Mask) -> Mask {
    let (w, h) = (m.w, m.h);
    let mut img = m.clone();
    // Clear the border so neighbourhood lookups stay in range.
    for x in 0..w {
        img.px[x] = 0;
        img.px[(h - 1) * w + x] = 0;
    }
    for y in 0..h {
        img.px[y * w] = 0;
        img.px[y * w + w - 1] = 0;
    }
    let mut active: Vec<usize> = (0..w * h).filter(|i| img.px[*i] != 0).collect();
    loop {
        let mut changed = false;
        for step in 0..2 {
            let mut del = Vec::new();
            for &i in &active {
                if img.px[i] == 0 {
                    continue;
                }
                let n = |o: isize| img.px[(i as isize + o) as usize];
                let wi = w as isize;
                let p = [n(-wi), n(-wi + 1), n(1), n(wi + 1), n(wi), n(wi - 1), n(-1), n(-wi - 1)];
                let b: u8 = p.iter().sum();
                if !(2..=6).contains(&b) {
                    continue;
                }
                let a = (0..8).filter(|k| p[*k] == 0 && p[(k + 1) % 8] == 1).count();
                if a != 1 {
                    continue;
                }
                let (p2, p4, p6, p8) = (p[0], p[2], p[4], p[6]);
                let ok = if step == 0 { p2 * p4 * p6 == 0 && p4 * p6 * p8 == 0 } else { p2 * p4 * p8 == 0 && p2 * p6 * p8 == 0 };
                if ok {
                    del.push(i);
                }
            }
            if !del.is_empty() {
                changed = true;
                for i in del {
                    img.px[i] = 0;
                }
            }
        }
        active.retain(|i| img.px[*i] != 0);
        if !changed {
            break;
        }
    }
    // Remove redundant staircase pixels (inner corners of diagonal steps).
    for &i in &active {
        let wi = w as isize;
        let g = |o: isize| img.px[(i as isize + o) as usize] != 0;
        let (n, e, s, wst) = (g(-wi), g(1), g(wi), g(-1));
        let (ne, se, sw, nw) = (g(-wi + 1), g(wi + 1), g(wi - 1), g(-wi - 1));
        let redundant = (n && e && !s && !wst && !sw)
            || (e && s && !n && !wst && !nw)
            || (s && wst && !n && !e && !ne)
            || (wst && n && !s && !e && !se);
        if redundant {
            img.px[i] = 0;
        }
    }
    img
}

/// Rotate a grayscale image 90° clockwise (for reading vertical text).
pub fn rotate90(g: &GrayImage) -> GrayImage {
    image::imageops::rotate90(g)
}
