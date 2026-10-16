// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Proteen Das

//! The DWG 2007 (AC1021) container: a file header and pages that are
//! Reed–Solomon coded (the data bytes interleaved with parity; the parity is
//! not needed to read an undamaged file) and compressed with a scheme of
//! their own, a page map, and a section map naming the sections.

fn le64(d: &[u8], at: usize) -> Option<u64> {
    Some(u64::from_le_bytes(d.get(at..at + 8)?.try_into().ok()?))
}

/// Undo the interleaving of `blocks` Reed–Solomon blocks of `size` data bytes.
fn deinterleave(src: &[u8], blocks: usize, size: usize) -> Option<Vec<u8>> {
    if blocks.checked_mul(size)? > src.len() {
        return None;
    }
    let mut out = vec![0u8; blocks * size];
    for i in 0..blocks {
        for j in 0..size {
            out[i * size + j] = *src.get(j * blocks + i)?;
        }
    }
    Some(out)
}

/// Literal runs are stored with their bytes shuffled in small groups.
fn copy_literal(dst: &mut Vec<u8>, src: &[u8]) {
    fn put(dst: &mut Vec<u8>, src: &[u8], n: usize, o: usize) {
        match n {
            1 => dst.push(src[o]),
            2 => dst.extend_from_slice(&[src[o + 1], src[o]]),
            3 => dst.extend_from_slice(&[src[o + 2], src[o + 1], src[o]]),
            16 => {
                dst.extend_from_slice(&src[o + 8..o + 16]);
                dst.extend_from_slice(&src[o..o + 8]);
            }
            _ => dst.extend_from_slice(&src[o..o + n]),
        }
    }
    const TAIL: [&[(usize, usize)]; 32] = [
        &[],
        &[(1, 0)],
        &[(2, 0)],
        &[(3, 0)],
        &[(4, 0)],
        &[(1, 4), (4, 0)],
        &[(1, 5), (4, 1), (1, 0)],
        &[(2, 5), (4, 1), (1, 0)],
        &[(8, 0)],
        &[(1, 8), (8, 0)],
        &[(1, 9), (8, 1), (1, 0)],
        &[(2, 9), (8, 1), (1, 0)],
        &[(4, 8), (8, 0)],
        &[(1, 12), (4, 8), (8, 0)],
        &[(1, 13), (4, 9), (8, 1), (1, 0)],
        &[(2, 13), (4, 9), (8, 1), (1, 0)],
        &[(16, 0)],
        &[(8, 9), (1, 8), (8, 0)],
        &[(1, 17), (16, 1), (1, 0)],
        &[(3, 16), (16, 0)],
        &[(4, 16), (16, 0)],
        &[(1, 20), (4, 16), (16, 0)],
        &[(2, 20), (4, 16), (16, 0)],
        &[(3, 20), (4, 16), (16, 0)],
        &[(8, 16), (16, 0)],
        &[(8, 17), (1, 16), (16, 0)],
        &[(1, 25), (8, 17), (1, 16), (16, 0)],
        &[(2, 25), (8, 17), (1, 16), (16, 0)],
        &[(4, 24), (8, 16), (16, 0)],
        &[(1, 28), (4, 24), (8, 16), (16, 0)],
        &[(2, 28), (4, 24), (8, 16), (16, 0)],
        &[(1, 30), (4, 26), (8, 18), (16, 2), (2, 0)],
    ];
    let mut s = src;
    while s.len() >= 32 {
        put(dst, s, 16, 16);
        put(dst, s, 16, 0);
        s = &s[32..];
    }
    for &(n, o) in TAIL[s.len()] {
        put(dst, s, n, o);
    }
}

/// The DWG 2007 compression.
pub fn decompress(src: &[u8], out_size: usize) -> Result<Vec<u8>, String> {
    let short = || "compressed data ends early".to_string();
    let mut dst: Vec<u8> = Vec::with_capacity(out_size);
    if src.len() < 2 {
        return Err(short());
    }
    let mut i = 0usize;
    let next = |i: &mut usize| -> Result<u8, String> {
        let v = *src.get(*i).ok_or_else(short)?;
        *i += 1;
        Ok(v)
    };
    let mut opcode = next(&mut i)?;
    let mut length: usize = 0;
    let mut offset: usize = 0;
    if opcode & 0xf0 == 0x20 {
        i += 2;
        length = (next(&mut i)? & 7) as usize;
        if length == 0 {
            return Err("damaged compressed data".into());
        }
    }
    let instructions = |i: &mut usize, opcode: &mut u8, offset: &mut usize, length: &mut usize| -> Result<(), String> {
        match *opcode >> 4 {
            0 => {
                *length = (*opcode & 0xf) as usize + 0x13;
                *offset = next(i)? as usize;
                *opcode = next(i)?;
                *length += ((*opcode >> 3) & 0x10) as usize;
                *offset += (((*opcode & 0x78) as usize) << 5) + 1;
            }
            1 => {
                *length = (*opcode & 0xf) as usize + 3;
                *offset = next(i)? as usize;
                *opcode = next(i)?;
                *offset += (((*opcode & 0xf8) as usize) << 5) + 1;
            }
            2 => {
                *offset = next(i)? as usize;
                *offset |= (next(i)? as usize) << 8;
                *length = (*opcode & 7) as usize;
                if *opcode & 8 == 0 {
                    *opcode = next(i)?;
                    *length += (*opcode & 0xf8) as usize;
                } else {
                    *offset += 1;
                    *length += (next(i)? as usize) << 3;
                    *opcode = next(i)?;
                    *length += (((*opcode & 0xf8) as usize) << 8) + 0x100;
                }
            }
            _ => {
                *length = (*opcode >> 4) as usize;
                *offset = (*opcode & 15) as usize;
                *opcode = next(i)?;
                *offset += (((*opcode & 0xf8) as usize) << 1) + 1;
            }
        }
        Ok(())
    };
    while i < src.len() {
        if length == 0 {
            length = opcode as usize + 8;
            if length == 0x17 {
                let n = next(&mut i)? as usize;
                length += n;
                if n == 0xff {
                    loop {
                        let n = next(&mut i)? as usize | (next(&mut i)? as usize) << 8;
                        length += n;
                        if n != 0xffff {
                            break;
                        }
                    }
                }
            }
        }
        let lit = src.get(i..i + length).ok_or_else(short)?;
        if dst.len() + length > out_size {
            return Err("compressed data runs past its size".into());
        }
        copy_literal(&mut dst, lit);
        i += length;
        length = 0;
        if i >= src.len() {
            break;
        }
        opcode = next(&mut i)?;
        instructions(&mut i, &mut opcode, &mut offset, &mut length)?;
        loop {
            if offset > dst.len() || dst.len() + length > out_size {
                return Err("damaged compressed data".into());
            }
            let from = dst.len() - offset;
            for k in 0..length {
                let b = dst[from + k];
                dst.push(b);
            }
            length = (opcode & 7) as usize;
            if length != 0 || i >= src.len() {
                break;
            }
            opcode = next(&mut i)?;
            if opcode >> 4 == 0 {
                break;
            }
            if opcode >> 4 == 0x0f {
                opcode &= 0xf;
            }
            instructions(&mut i, &mut opcode, &mut offset, &mut length)?;
        }
    }
    dst.resize(out_size, 0);
    Ok(dst)
}

/// A system page (page map, section map): RS blocks of 239 data bytes.
fn system_page(file: &[u8], at: usize, comp: usize, uncomp: usize, repeat: usize) -> Result<Vec<u8>, String> {
    let pesize = ((comp + 7) & !7) * repeat.max(1);
    let blocks = pesize.div_ceil(239);
    let page_size = (blocks * 255 + 7) & !7;
    let raw = file.get(at..at + page_size).ok_or("a system page lies outside the file")?;
    let data = deinterleave(raw, blocks, 239).ok_or("damaged system page")?;
    if comp < uncomp {
        decompress(&data[..comp.min(data.len())], uncomp)
    } else {
        Ok(data.get(..uncomp).ok_or("damaged system page")?.to_vec())
    }
}

struct Section {
    name: String,
    size: usize,
    /// (page id, offset in the section, compressed size, size)
    pages: Vec<(u64, usize, usize, usize)>,
}

/// The named sections of a DWG 2007 file, read on demand.
pub struct File2007<'a> {
    file: &'a [u8],
    /// page id → (offset in the file, size on disk)
    pages: Vec<(u64, usize, usize)>,
    sections: Vec<Section>,
}

impl<'a> File2007<'a> {
    pub fn open(file: &'a [u8]) -> Result<File2007<'a>, String> {
        let raw = file.get(0x80..0x80 + 0x3d8).ok_or("damaged file header")?;
        let pe = deinterleave(raw, 3, 239).ok_or("damaged file header")?;
        let clen = i32::from_le_bytes(pe[24..28].try_into().unwrap());
        let head =
            if clen > 0 { decompress(&pe[32..32 + (clen as usize).min(pe.len() - 32)], 34 * 8)? } else { pe[32..32 + 34 * 8].to_vec() };
        let f = |k: usize| le64(&head, k * 8).unwrap_or(0) as usize;
        let (pm_off, pm_comp, pm_uncomp, pm_corr) = (f(7), f(10), f(11), f(3));
        let (sm_comp, sm_id, sm_uncomp, sm_corr) = (f(22), f(24) as u64, f(25), f(27));
        if pm_comp > file.len() || pm_uncomp > 1 << 28 || sm_uncomp > 1 << 28 {
            return Err("damaged file header".into());
        }
        let pm = system_page(file, 0x480 + pm_off, pm_comp, pm_uncomp, pm_corr)?;
        let mut pages = vec![];
        let mut offset = 0x480usize;
        let mut at = 0;
        while at + 16 <= pm.len() {
            let size = le64(&pm, at).unwrap_or(0) as usize;
            let id = le64(&pm, at + 8).unwrap_or(0);
            pages.push((id, offset, size));
            offset = offset.saturating_add(size);
            at += 16;
        }
        let &(_, sm_at, _) = pages.iter().find(|p| p.0 == sm_id).ok_or("the section map is missing")?;
        let sm = system_page(file, sm_at, sm_comp, sm_uncomp, sm_corr)?;
        let mut sections = vec![];
        let mut at = 0;
        while at + 64 <= sm.len() {
            let g = |k: usize| le64(&sm, at + k * 8).unwrap_or(0) as usize;
            let (size, name_len, n) = (g(0), g(4), g(7));
            at += 64;
            if name_len >= 48 || size > 10 * file.len() {
                break;
            }
            let name_len = name_len + (name_len & 1);
            let units: Vec<u16> = sm
                .get(at..at + name_len)
                .unwrap_or_default()
                .chunks(2)
                .map(|c| u16::from_le_bytes([c[0], *c.get(1).unwrap_or(&0)]))
                .collect();
            let name = String::from_utf16_lossy(&units).trim_end_matches('\0').to_string();
            at += name_len;
            let mut pg = vec![];
            for _ in 0..n.min(0xf0000) {
                if at + 56 > sm.len() {
                    break;
                }
                let h = |k: usize| le64(&sm, at + k * 8).unwrap_or(0) as usize;
                pg.push((h(2) as u64, h(0), h(4), h(3)));
                at += 56;
            }
            sections.push(Section { name, size, pages: pg });
        }
        Ok(File2007 { file, pages, sections })
    }

    pub fn section(&self, name: &str) -> Result<Vec<u8>, String> {
        let s = self.sections.iter().find(|s| s.name == name).ok_or_else(|| format!("the drawing has no {name} section"))?;
        if s.size > 1 << 30 {
            return Err(format!("the {name} section is implausibly large"));
        }
        let mut buf = vec![0u8; s.size];
        for &(id, offset, comp, uncomp) in &s.pages {
            let &(_, at, disk) = self.pages.iter().find(|p| p.0 == id).ok_or("a section page is missing")?;
            if offset + uncomp > buf.len() {
                return Err(format!("a page of the {name} section is damaged"));
            }
            let pesize = (comp + 7) & !7;
            let blocks = pesize.div_ceil(0xfb);
            let rs_size = (blocks * 0xff + 31) & !31;
            let chunk = if comp != uncomp || disk == rs_size {
                let raw = self.file.get(at..at + disk).ok_or("a section page lies outside the file")?;
                let data = deinterleave(raw, blocks, 0xfb).ok_or("damaged section page")?;
                if comp < uncomp {
                    decompress(&data[..comp.min(data.len())], uncomp)?
                } else {
                    data.get(..uncomp).ok_or("damaged section page")?.to_vec()
                }
            } else {
                self.file.get(at..at + uncomp).ok_or("a section page lies outside the file")?.to_vec()
            };
            buf[offset..offset + uncomp].copy_from_slice(&chunk[..uncomp]);
        }
        Ok(buf)
    }
}
