// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Proteen Das

//! The class section: object types numbered 500 and up, by DXF name
//! (IMAGE, IMAGEDEF, XRECORD, LAYOUT…).

use super::bits::{Bits, Ver};
use super::container::Container;
use std::collections::HashMap;

const SENTINEL: [u8; 16] = [0x8D, 0xA1, 0xC4, 0xB8, 0xC4, 0xA9, 0xF8, 0xC5, 0xC0, 0xDC, 0xF4, 0x5F, 0xE7, 0xCF, 0xB6, 0x8A];

/// Class number → (DXF name, whether its objects are entities).
pub fn read(c: &Container) -> HashMap<u16, (String, bool)> {
    let mut out = HashMap::new();
    let data = &c.classes;
    let Some(start) = data.windows(16).take(64).position(|w| w == SENTINEL) else { return out };
    let ver = c.ver;
    let mut d = Bits::new(data, ver);
    d.pos = (start + 16) * 8;
    let size = d.rl() as usize;
    let end = (start + 16 + 4 + size).min(data.len());
    if ver < Ver::R2004 {
        while d.pos / 8 + 1 < end && !d.overrun {
            let number = d.bs();
            d.bs();
            d.t();
            d.t();
            let dxf = d.t();
            d.b();
            let id = d.bs();
            if d.overrun {
                break;
            }
            out.insert(number, (dxf, id == 0x1f2));
        }
        return out;
    }
    let mut bitsize = 0usize;
    if (ver >= Ver::R2010 && c.maint > 3) || ver >= Ver::R2018 {
        d.rl();
    }
    if ver >= Ver::R2007 {
        bitsize = d.rl() as usize;
    }
    let max = d.bs();
    d.rc();
    d.rc();
    d.b();
    let mut strings = None;
    if ver >= Ver::R2007 {
        // The strings come from a stream at the end of the class data.
        let base = start * 8;
        let at = base + bitsize + if ver < Ver::R2010 || (ver == Ver::R2010 && c.maint < 3) { 159 } else { 191 };
        let mut r = d.sub(at, at + 1);
        if r.b() && at >= 16 {
            let mut p = at - 16;
            let mut rs = d.sub(p, p + 16);
            let mut n = rs.rs() as usize;
            if n & 0x8000 != 0 && p >= 16 {
                p -= 16;
                let mut hi = d.sub(p, p + 16);
                n = (n & 0x7fff) | (hi.rs() as usize) << 15;
            }
            if n <= p {
                strings = Some(d.sub(p - n, p));
            }
        }
    }
    for _ in 0..max.saturating_sub(499) {
        let number = d.bs();
        d.bs();
        let dxf = match &mut strings {
            Some(s) => {
                s.t();
                s.t();
                s.t()
            }
            None => {
                d.t();
                d.t();
                d.t()
            }
        };
        d.b();
        let id = d.bs();
        d.bl();
        d.bs();
        d.bs();
        d.bl();
        d.bl();
        if d.overrun {
            break;
        }
        out.insert(number, (dxf, id == 0x1f2));
    }
    out
}
