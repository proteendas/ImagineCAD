// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Proteen Das

//! The header variables a 2D drawing needs: the linetype scale and the
//! dimension variables that size annotation (DIMSCALE, DIMASZ, DIMEXO,
//! DIMEXE, DIMTXT, DIMGAP, DIMDEC). The variables are a fixed sequence;
//! everything before these is read and passed over.

use super::bits::{Bits, Ver};
use super::container::Container;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Vars {
    pub ltscale: f64,
    pub dimscale: f64,
    pub dimasz: f64,
    pub dimexo: f64,
    pub dimexe: f64,
    pub dimtxt: f64,
    pub dimgap: f64,
    pub dimdec: u16,
}

const SENTINEL: [u8; 16] = [0xCF, 0x7B, 0x1F, 0x23, 0xFD, 0xDE, 0x38, 0xA9, 0x5F, 0x7C, 0x68, 0xB8, 0x4E, 0x6D, 0x33, 0x5F];

pub fn read(c: &Container) -> Option<Vars> {
    let data = &c.header;
    let start = data.windows(16).take(64).position(|w| w == SENTINEL)? + 16;
    let ver = c.ver;
    let mut d = Bits::new(data, ver);
    d.pos = start * 8;
    let _size = d.rl();
    if ver >= Ver::R2007 {
        if (ver >= Ver::R2010 && c.maint > 3) || ver >= Ver::R2018 {
            d.rl();
        }
        d.rl();
    }
    // Before R2007 handles are inline; since, they have their own stream.
    let inline = ver < Ver::R2007;
    let h = |d: &mut Bits| {
        if inline {
            d.h();
        }
    };
    if ver >= Ver::R2013 {
        d.bll();
    }
    for _ in 0..4 {
        d.bd();
    }
    if ver <= Ver::R2004 {
        for _ in 0..4 {
            d.t();
        }
    }
    d.bl();
    d.bl();
    if ver == Ver::R2000 {
        h(&mut d);
    }
    let bits = 2 + 7 + usize::from(ver >= Ver::R2004) + 4 + 2 + 3 + 2;
    for _ in 0..bits {
        d.b();
    }
    for _ in 0..(1 + 1 + 4 + 1 + 1) {
        d.bs();
    }
    if ver >= Ver::R2004 {
        for _ in 0..3 {
            d.bl();
        }
    }
    for _ in 0..19 {
        d.bs();
    }
    let ltscale = d.bd();
    for _ in 0..20 {
        d.bd();
    }
    if ver < Ver::R2007 {
        d.t();
    }
    for _ in 0..4 {
        d.bl();
    }
    if ver >= Ver::R2004 {
        for _ in 0..3 {
            d.bl();
        }
    }
    for _ in 0..4 {
        d.bl();
    }
    d.cmc(None);
    d.h(); // HANDSEED, always in the data
    for _ in 0..3 {
        h(&mut d);
    }
    if ver >= Ver::R2007 {
        h(&mut d);
    }
    h(&mut d);
    h(&mut d);
    d.bd();
    // Paper space: insertion base, extents, limits, elevation, UCS.
    d.p3bd();
    d.p3bd();
    d.p3bd();
    d.p2rd();
    d.p2rd();
    d.bd();
    d.p3bd();
    d.p3bd();
    d.p3bd();
    h(&mut d);
    h(&mut d);
    d.bs();
    h(&mut d);
    for _ in 0..6 {
        d.p3bd();
    }
    // Model space, the same.
    d.p3bd();
    d.p3bd();
    d.p3bd();
    d.p2rd();
    d.p2rd();
    d.bd();
    d.p3bd();
    d.p3bd();
    d.p3bd();
    h(&mut d);
    h(&mut d);
    d.bs();
    h(&mut d);
    for _ in 0..6 {
        d.p3bd();
    }
    if ver < Ver::R2007 {
        d.t();
        d.t();
    }
    let dimscale = d.bd();
    let dimasz = d.bd();
    let dimexo = d.bd();
    d.bd();
    let dimexe = d.bd();
    for _ in 0..4 {
        d.bd();
    }
    if ver >= Ver::R2007 {
        d.bd();
        d.bd();
        d.bs();
        d.cmc(None);
    }
    for _ in 0..6 {
        d.b();
    }
    for _ in 0..3 {
        d.bs();
    }
    if ver >= Ver::R2007 {
        d.bs();
    }
    let dimtxt = d.bd();
    for _ in 0..6 {
        d.bd();
    }
    let dimgap = d.bd();
    d.bd();
    d.b();
    d.bs();
    for _ in 0..4 {
        d.b();
    }
    for _ in 0..3 {
        d.cmc(None);
    }
    d.bs();
    let dimdec = d.bs();
    let v = Vars { ltscale, dimscale, dimasz, dimexo, dimexe, dimtxt, dimgap, dimdec };
    let sane = |x: f64, lo: f64, hi: f64| x.is_finite() && x >= lo && x <= hi;
    if d.overrun
        || !sane(dimscale, 0.0, 1e6)
        || !sane(dimtxt, 1e-6, 1e5)
        || !sane(dimasz, 0.0, 1e5)
        || !sane(ltscale, 1e-9, 1e9)
        || dimdec > 8
    {
        return None;
    }
    Some(v)
}
