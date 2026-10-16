// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Proteen Das

//! The bit-coded values DWG objects are made of (Open Design Specification
//! for .dwg files, chapter 2: B, BB, BS, BL, BD, DD, MC, MS, H, T…). Reads
//! past the end yield zeros and set `overrun`, so a damaged object is caught
//! by its caller instead of panicking.

/// DWG releases the reader distinguishes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Ver {
    R2000,
    R2004,
    R2007,
    R2010,
    R2013,
    R2018,
}

impl Ver {
    pub fn from_magic(m: &[u8]) -> Option<Ver> {
        Some(match m.get(..6)? {
            b"AC1015" => Ver::R2000,
            b"AC1018" => Ver::R2004,
            b"AC1021" => Ver::R2007,
            b"AC1024" => Ver::R2010,
            b"AC1027" => Ver::R2013,
            b"AC1032" => Ver::R2018,
            _ => return None,
        })
    }
}

/// An object handle as stored: reference code, and the value.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Handle {
    pub code: u8,
    pub value: u64,
}

impl Handle {
    /// The absolute handle this reference names, seen from object `from`.
    pub fn absolute(&self, from: u64) -> u64 {
        match self.code {
            6 => from.wrapping_add(1),
            8 => from.wrapping_sub(1),
            0xA => from.wrapping_add(self.value),
            0xC => from.wrapping_sub(self.value),
            _ => self.value,
        }
    }
}

#[derive(Clone)]
pub struct Bits<'a> {
    pub data: &'a [u8],
    /// Current position in bits.
    pub pos: usize,
    /// End in bits (exclusive).
    pub end: usize,
    pub ver: Ver,
    pub overrun: bool,
}

impl<'a> Bits<'a> {
    pub fn new(data: &'a [u8], ver: Ver) -> Bits<'a> {
        Bits { data, pos: 0, end: data.len() * 8, ver, overrun: false }
    }

    /// A reader over bits `start..end` of the same data.
    pub fn sub(&self, start: usize, end: usize) -> Bits<'a> {
        Bits { data: self.data, pos: start, end: end.min(self.data.len() * 8), ver: self.ver, overrun: false }
    }

    pub fn left(&self) -> isize {
        self.end as isize - self.pos as isize
    }

    #[inline]
    pub fn b(&mut self) -> bool {
        if self.pos >= self.end {
            self.overrun = true;
            self.pos += 1;
            return false;
        }
        let v = self.data[self.pos / 8] >> (7 - self.pos % 8) & 1;
        self.pos += 1;
        v == 1
    }

    /// `n` bits, most significant first.
    pub fn bits(&mut self, n: u32) -> u64 {
        let mut v = 0u64;
        for _ in 0..n {
            v = v << 1 | self.b() as u64;
        }
        v
    }

    pub fn bb(&mut self) -> u8 {
        self.bits(2) as u8
    }

    pub fn b3(&mut self) -> u8 {
        // 3B: up to three bits, stopping at the first zero.
        let mut v = 0;
        for _ in 0..3 {
            if !self.b() {
                break;
            }
            v = v << 1 | 1;
        }
        v
    }

    pub fn rc(&mut self) -> u8 {
        if self.pos.is_multiple_of(8) && self.pos + 8 <= self.end {
            let v = self.data[self.pos / 8];
            self.pos += 8;
            return v;
        }
        self.bits(8) as u8
    }

    pub fn rs(&mut self) -> u16 {
        let lo = self.rc() as u16;
        let hi = self.rc() as u16;
        lo | hi << 8
    }

    pub fn rl(&mut self) -> u32 {
        let lo = self.rs() as u32;
        let hi = self.rs() as u32;
        lo | hi << 16
    }

    pub fn rll(&mut self) -> u64 {
        let lo = self.rl() as u64;
        let hi = self.rl() as u64;
        lo | hi << 32
    }

    pub fn rd(&mut self) -> f64 {
        f64::from_bits(self.rll())
    }

    pub fn bs(&mut self) -> u16 {
        match self.bb() {
            0 => self.rs(),
            1 => self.rc() as u16,
            2 => 0,
            _ => 256,
        }
    }

    pub fn bl(&mut self) -> u32 {
        match self.bb() {
            0 => self.rl(),
            1 => self.rc() as u32,
            _ => 0,
        }
    }

    pub fn bd(&mut self) -> f64 {
        match self.bb() {
            0 => self.rd(),
            1 => 1.0,
            2 => 0.0,
            _ => {
                self.overrun = true;
                0.0
            }
        }
    }

    /// Bit double with a default: unchanged, low 4 bytes, 6 bytes, or new.
    pub fn dd(&mut self, default: f64) -> f64 {
        match self.bb() {
            0 => default,
            1 => {
                let mut b = default.to_le_bytes();
                for x in b.iter_mut().take(4) {
                    *x = self.rc();
                }
                f64::from_le_bytes(b)
            }
            2 => {
                let mut b = default.to_le_bytes();
                b[4] = self.rc();
                b[5] = self.rc();
                for x in b.iter_mut().take(4) {
                    *x = self.rc();
                }
                f64::from_le_bytes(b)
            }
            _ => self.rd(),
        }
    }

    /// Bit long long: a 3-bit byte count, then that many bytes (least
    /// significant first).
    pub fn bll(&mut self) -> u64 {
        let n = self.bits(3);
        (0..n).fold(0u64, |v, i| v | (self.rc() as u64) << (8 * i))
    }

    /// Bit-coded object type (R2010+).
    pub fn bot(&mut self) -> u16 {
        match self.bb() {
            0 => self.rc() as u16,
            1 => self.rc() as u16 + 0x1f0,
            _ => self.rs(),
        }
    }

    /// Modular char, signed.
    pub fn mc(&mut self) -> i64 {
        let mut result: u64 = 0;
        let mut shift = 0;
        for _ in 0..8 {
            let mut c = self.rc();
            if c & 0x80 == 0 {
                let neg = c & 0x40 != 0;
                c &= 0xbf;
                result |= (c as u64) << shift;
                return if neg { -(result as i64) } else { result as i64 };
            }
            result |= ((c & 0x7f) as u64) << shift;
            shift += 7;
        }
        self.overrun = true;
        0
    }

    /// Modular char, unsigned.
    pub fn umc(&mut self) -> u64 {
        let mut result: u64 = 0;
        let mut shift = 0;
        for _ in 0..9 {
            let c = self.rc();
            result |= ((c & 0x7f) as u64) << shift;
            if c & 0x80 == 0 {
                return result;
            }
            shift += 7;
        }
        self.overrun = true;
        0
    }

    /// Modular short.
    pub fn ms(&mut self) -> u32 {
        let mut result: u32 = 0;
        let mut shift = 0;
        for _ in 0..2 {
            let w = self.rs() as u32;
            result |= (w & 0x7fff) << shift;
            if w & 0x8000 == 0 {
                return result;
            }
            shift += 15;
        }
        result
    }

    pub fn h(&mut self) -> Handle {
        let c = self.rc();
        let size = c & 0xf;
        let code = c >> 4;
        let mut value = 0u64;
        if size > 8 {
            self.overrun = true;
            return Handle::default();
        }
        for _ in 0..size {
            value = value << 8 | self.rc() as u64;
        }
        Handle { code, value }
    }

    /// Text: 8-bit before R2007 (as Windows-1252), UTF-16 since.
    pub fn t(&mut self) -> String {
        let n = self.bs() as usize;
        if self.ver >= Ver::R2007 {
            if n * 16 > self.left().max(0) as usize {
                self.overrun = true;
                return String::new();
            }
            let units: Vec<u16> = (0..n).map(|_| self.rs()).collect();
            let s = String::from_utf16_lossy(&units);
            s.trim_end_matches('\0').to_string()
        } else {
            if n * 8 > self.left().max(0) as usize {
                self.overrun = true;
                return String::new();
            }
            let bytes: Vec<u8> = (0..n).map(|_| self.rc()).collect();
            cp1252(&bytes)
        }
    }

    /// 2RD
    pub fn p2rd(&mut self) -> (f64, f64) {
        (self.rd(), self.rd())
    }

    /// 3BD
    pub fn p3bd(&mut self) -> (f64, f64, f64) {
        (self.bd(), self.bd(), self.bd())
    }

    /// 2DD with defaults
    pub fn p2dd(&mut self, d: (f64, f64)) -> (f64, f64) {
        let x = self.dd(d.0);
        let y = self.dd(d.1);
        (x, y)
    }

    /// Bit thickness.
    pub fn bt(&mut self) -> f64 {
        if self.ver >= Ver::R2000 && self.b() {
            0.0
        } else {
            self.bd()
        }
    }

    /// Bit extrusion.
    pub fn be(&mut self) -> (f64, f64, f64) {
        if self.ver >= Ver::R2000 && self.b() {
            (0.0, 0.0, 1.0)
        } else {
            self.p3bd()
        }
    }

    /// Colour of a table record (CMC): the ACI index (negative = layer off
    /// before R2004), and for R2004+ a true colour.
    pub fn cmc(&mut self, strings: Option<&mut Bits>) -> Color {
        let index = self.bs() as i16;
        if self.ver < Ver::R2004 {
            return Color { index, rgb: None };
        }
        let rgb = self.bl();
        let flag = self.rc();
        if flag < 4 {
            match strings {
                Some(s) => {
                    if flag & 1 != 0 {
                        s.t();
                    }
                    if flag & 2 != 0 {
                        s.t();
                    }
                }
                None => {
                    if flag & 1 != 0 {
                        self.t();
                    }
                    if flag & 2 != 0 {
                        self.t();
                    }
                }
            }
        }
        color_from_rgb_method(index, rgb)
    }

    /// Entity colour (ENC, R2004+); the colour book handle, when there is
    /// one, sits in the handle stream and is flagged in `book`.
    pub fn enc(&mut self) -> (Color, bool) {
        let raw = self.bs();
        if self.ver < Ver::R2004 {
            return (Color { index: raw as i16, rgb: None }, false);
        }
        let flag = (raw >> 8) as u8;
        let index = (raw & 0x1ff) as i16;
        let mut rgb = None;
        if flag & 0x80 != 0 {
            let v = self.bl();
            rgb = Some(v);
        }
        if flag & 0x20 != 0 {
            self.bl();
        }
        let c = match rgb {
            Some(v) => color_from_rgb_method(index, v),
            None => Color { index, rgb: None },
        };
        (c, flag & 0x40 != 0)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Color {
    /// ACI: 0 ByBlock, 256 ByLayer.
    pub index: i16,
    pub rgb: Option<u32>,
}

fn color_from_rgb_method(index: i16, rgb: u32) -> Color {
    match rgb >> 24 {
        0xc0 => Color { index: 256, rgb: None },
        0xc1 => Color { index: 0, rgb: None },
        0xc3 => Color { index: (rgb & 0xff) as i16, rgb: None },
        0xc2 => Color { index, rgb: Some(rgb & 0xff_ffff) },
        _ => Color { index, rgb: None },
    }
}

/// Windows-1252 bytes as text (the usual code page of R2000–R2004 drawings).
pub fn cp1252(bytes: &[u8]) -> String {
    const HIGH: [char; 32] = [
        '€', '\u{81}', '‚', 'ƒ', '„', '…', '†', '‡', 'ˆ', '‰', 'Š', '‹', 'Œ', '\u{8d}', 'Ž', '\u{8f}', '\u{90}', '‘', '’', '“', '”', '•',
        '–', '—', '˜', '™', 'š', '›', 'œ', '\u{9d}', 'ž', 'Ÿ',
    ];
    bytes
        .iter()
        .take_while(|b| **b != 0)
        .map(|&b| match b {
            0x80..=0x9f => HIGH[(b - 0x80) as usize],
            _ => b as char,
        })
        .collect()
}

/// CRC-16 used by DWG sections (CRC-16/ARC, seeded with 0xC0C1).
pub fn crc16(seed: u16, data: &[u8]) -> u16 {
    let mut crc = seed;
    for &b in data {
        crc ^= b as u16;
        for _ in 0..8 {
            crc = if crc & 1 != 0 { crc >> 1 ^ 0xA001 } else { crc >> 1 };
        }
    }
    crc
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bit_codes() {
        // BS 10 (=0) then BS 01 + 0x05, BD 01 (=1.0), BB 11
        let data = [0b1001_0000, 0b0101_0111, 0b0000_0000];
        let mut b = Bits::new(&data, Ver::R2000);
        assert_eq!(b.bs(), 0);
        assert_eq!(b.bs(), 5);
        assert_eq!(b.bd(), 1.0);
        assert_eq!(b.bb(), 3);
        // Modular chars: 0x82 0x01 = 130; 0x41 = -1.
        let mut m = Bits::new(&[0x82, 0x01, 0x41], Ver::R2000);
        assert_eq!(m.mc(), 130);
        assert_eq!(m.mc(), -1);
        // A handle: code 5, two bytes 0x01 0x2F.
        let mut h = Bits::new(&[0x52, 0x01, 0x2f], Ver::R2000);
        assert_eq!(h.h(), Handle { code: 5, value: 0x12f });
        assert_eq!(Handle { code: 0xA, value: 3 }.absolute(10), 13);
        // Reading past the end is flagged, not a panic.
        let mut e = Bits::new(&[0], Ver::R2000);
        e.rl();
        assert!(e.overrun);
        // CRC-16/ARC check value.
        assert_eq!(crc16(0, b"123456789"), 0xBB3D);
    }
}
