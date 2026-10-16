// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Proteen Das

//! The DWG file container: where the object map and the objects are.
//!
//! * R2000 (AC1015): a table of section locators after the file header; the
//!   object map gives absolute file offsets.
//! * R2004 and R2010+ (AC1018, AC1024, AC1027, AC1032): an encrypted file
//!   header, a page map and a section map, and sections cut into pages that
//!   are each compressed (an LZ77 variant); the object map gives offsets in
//!   the `AcDb:AcDbObjects` section.
//!
//! * R2007 (AC1021): Reed–Solomon coded pages with their own compression
//!   (see `r2007.rs`), and the same sections inside.

use super::bits::{Bits, Ver};

pub struct Container {
    pub ver: Ver,
    pub maint: u8,
    /// Where the objects are (the whole file before R2004).
    pub objects: Vec<u8>,
    /// (handle, offset into `objects`), in map order.
    pub map: Vec<(u64, usize)>,
    /// The header variables section.
    pub header: Vec<u8>,
    /// The class section (types numbered 500 and up).
    pub classes: Vec<u8>,
}

fn rl(d: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(d.get(at..at + 4)?.try_into().ok()?))
}

fn rll(d: &[u8], at: usize) -> Option<u64> {
    Some(u64::from_le_bytes(d.get(at..at + 8)?.try_into().ok()?))
}

pub fn read(file: &[u8]) -> Result<Container, String> {
    let ver = Ver::from_magic(file).ok_or_else(|| {
        let magic = String::from_utf8_lossy(file.get(..6).unwrap_or_default()).to_string();
        format!("not a DWG file of a supported version ({magic})")
    })?;
    if file.len() < 0x100 {
        return Err("the file is too short to be a drawing".into());
    }
    let maint = file[0x0b];
    match ver {
        Ver::R2000 => read_r2000(file, ver, maint),
        Ver::R2007 => read_r2007(file, ver, maint),
        _ => read_r2004(file, ver, maint),
    }
}

/// The object map: sections of (handle delta, offset delta) pairs.
fn object_map(data: &[u8]) -> Vec<(u64, usize)> {
    let mut out = vec![];
    let mut at = 0usize;
    while at + 2 <= data.len() {
        let size = u16::from_be_bytes([data[at], data[at + 1]]) as usize;
        if size <= 2 || at + size > data.len() {
            break;
        }
        let mut b = Bits::new(&data[at + 2..at + size], Ver::R2000);
        let (mut handle, mut offset) = (0u64, 0i64);
        while b.left() >= 16 && !b.overrun {
            let dh = b.umc();
            let doff = b.mc();
            if b.overrun {
                break;
            }
            handle = handle.wrapping_add(dh);
            offset += doff;
            if offset >= 0 {
                out.push((handle, offset as usize));
            }
        }
        // Section, then a big-endian CRC.
        at += size + 2;
    }
    out
}

fn read_r2000(file: &[u8], ver: Ver, maint: u8) -> Result<Container, String> {
    let n = rl(file, 0x15).ok_or("damaged file header")? as usize;
    if n > 16 {
        return Err("damaged file header (section count)".into());
    }
    let mut sections = vec![];
    for i in 0..n {
        let at = 0x19 + i * 9;
        let number = *file.get(at).ok_or("damaged file header")?;
        let address = rl(file, at + 1).ok_or("damaged file header")? as usize;
        let size = rl(file, at + 5).ok_or("damaged file header")? as usize;
        sections.push((number, address, size));
    }
    let (_, address, size) = *sections.iter().find(|s| s.0 == 2).ok_or("the drawing has no object map")?;
    let handles = file.get(address..address + size).ok_or("the object map lies outside the file")?;
    let part = |k: u8| sections.iter().find(|s| s.0 == k).and_then(|&(_, a, n)| file.get(a..a + n)).unwrap_or_default().to_vec();
    Ok(Container { ver, maint, map: object_map(handles), objects: file.to_vec(), header: part(0), classes: part(1) })
}

/// The R2004 file header is XORed with a pseudo-random sequence.
fn decrypt_header(src: &[u8]) -> Vec<u8> {
    let mut seed: u32 = 1;
    src.iter()
        .map(|b| {
            seed = seed.wrapping_mul(0x343fd).wrapping_add(0x269ec3);
            b ^ (seed >> 16) as u8
        })
        .collect()
}

/// The LZ77 variant of R2004+ sections.
pub fn decompress(src: &[u8], out_size: usize) -> Result<Vec<u8>, String> {
    let mut out: Vec<u8> = Vec::with_capacity(out_size);
    let mut i = 0usize;
    let next = |i: &mut usize| -> Result<u8, String> {
        let v = *src.get(*i).ok_or("compressed data ends early")?;
        *i += 1;
        Ok(v)
    };
    let literal_length = |i: &mut usize, op: u8| -> Result<usize, String> {
        let mut n = (op & 0xf) as usize;
        if n == 0 {
            loop {
                let b = *src.get(*i).ok_or("compressed data ends early")?;
                *i += 1;
                if b != 0 {
                    n += 0xf + b as usize;
                    break;
                }
                n += 0xff;
            }
        }
        Ok(n + 3)
    };
    let long_count = |i: &mut usize, op: u8, bits: u8| -> Result<usize, String> {
        let mut n = (op & bits) as usize;
        if n == 0 {
            loop {
                let b = *src.get(*i).ok_or("compressed data ends early")?;
                *i += 1;
                if b != 0 {
                    n += b as usize + bits as usize;
                    break;
                }
                n += 0xff;
            }
        }
        Ok(n + 2)
    };
    let mut op = next(&mut i)?;
    if op & 0xf0 == 0 {
        let n = literal_length(&mut i, op)?;
        out.extend_from_slice(src.get(i..i + n).ok_or("compressed data ends early")?);
        i += n;
        op = next(&mut i)?;
    }
    while op != 0x11 && out.len() < out_size {
        let (count, offset);
        if !(0x10..0x40).contains(&op) {
            count = ((op >> 4) as usize).saturating_sub(1);
            let op2 = next(&mut i)?;
            offset = ((op >> 2 & 3) as usize | (op2 as usize) << 2) + 1;
        } else if op < 0x20 {
            count = long_count(&mut i, op, 7)?;
            let b1 = next(&mut i)?;
            let b2 = next(&mut i)?;
            offset = (((op & 8) as usize) << 11) + ((b1 >> 2) as usize | (b2 as usize) << 6) + 0x4000;
            op = b1;
        } else {
            count = long_count(&mut i, op, 0x1f)?;
            let b1 = next(&mut i)?;
            let b2 = next(&mut i)?;
            offset = ((b1 >> 2) as usize | (b2 as usize) << 6) + 1;
            op = b1;
        }
        if offset > out.len() {
            return Err("compressed data refers before its start".into());
        }
        let from = out.len() - offset;
        for k in 0..count {
            let v = out[from + k];
            out.push(v);
        }
        let mut lit = (op & 3) as usize;
        if lit == 0 {
            op = next(&mut i)?;
            if op & 0xf0 == 0 {
                lit = literal_length(&mut i, op)?;
            }
        }
        if lit > 0 {
            out.extend_from_slice(src.get(i..i + lit).ok_or("compressed data ends early")?);
            i += lit;
            op = next(&mut i)?;
        }
    }
    out.resize(out_size, 0);
    Ok(out)
}

fn read_r2007(file: &[u8], ver: Ver, maint: u8) -> Result<Container, String> {
    let f = super::r2007::File2007::open(file)?;
    let objects = f.section("AcDb:AcDbObjects")?;
    let handles = f.section("AcDb:Handles")?;
    let header = f.section("AcDb:Header").unwrap_or_default();
    let classes = f.section("AcDb:Classes").unwrap_or_default();
    Ok(Container { ver, maint, map: object_map(&handles), objects, header, classes })
}

struct Page {
    number: i32,
    address: usize,
}

struct SectionDesc {
    name: String,
    size: u64,
    max_decomp: usize,
    compressed: bool,
    encrypted: u32,
    /// (page number, start offset in the section)
    pages: Vec<(i32, u64)>,
}

fn read_r2004(file: &[u8], ver: Ver, maint: u8) -> Result<Container, String> {
    let hdr = decrypt_header(file.get(0x80..0x80 + 0x6c).ok_or("damaged file header")?);
    if !hdr.starts_with(b"AcFssFcAJMB") {
        return Err("damaged or encrypted file header".into());
    }
    let map_address = rll(&hdr, 0x54).ok_or("damaged file header")? as usize + 0x100;
    let info_id = rl(&hdr, 0x5c).ok_or("damaged file header")? as i32;

    // Page map: page numbers and sizes; addresses follow from the sizes.
    let ty = rl(file, map_address).ok_or("the page map lies outside the file")?;
    if ty != 0x41630e3b {
        return Err("the page map is damaged".into());
    }
    let decomp = rl(file, map_address + 4).unwrap_or(0) as usize;
    let comp = rl(file, map_address + 8).unwrap_or(0) as usize;
    let raw = file.get(map_address + 20..map_address + 20 + comp).ok_or("the page map lies outside the file")?;
    let map = decompress(raw, decomp)?;
    let mut pages = vec![];
    let mut address = 0x100usize;
    let mut at = 0;
    while at + 8 <= map.len() {
        let number = rl(&map, at).unwrap_or(0) as i32;
        let size = rl(&map, at + 4).unwrap_or(0) as usize;
        at += 8;
        pages.push(Page { number, address });
        address += size;
        if number < 0 {
            at += 16;
        }
    }
    let page_at = |n: i32| pages.iter().find(|p| p.number == n).map(|p| p.address);

    // Section map: the named sections and their pages.
    let info_at = page_at(info_id).ok_or("the section map is missing")?;
    if rl(file, info_at) != Some(0x4163003b) {
        return Err("the section map is damaged".into());
    }
    let decomp = rl(file, info_at + 4).unwrap_or(0) as usize;
    let comp = rl(file, info_at + 8).unwrap_or(0) as usize;
    let info = decompress(file.get(info_at + 20..info_at + 20 + comp).ok_or("the section map lies outside the file")?, decomp)?;
    let count = rl(&info, 0).unwrap_or(0) as usize;
    let mut descs = vec![];
    let mut at = 20;
    for _ in 0..count.min(256) {
        let size = rll(&info, at).ok_or("the section map is damaged")?;
        let n = rl(&info, at + 8).unwrap_or(0) as usize;
        let max_decomp = rl(&info, at + 12).unwrap_or(0) as usize;
        let compressed = rl(&info, at + 20).unwrap_or(0) == 2;
        let encrypted = rl(&info, at + 28).unwrap_or(0);
        let name_bytes = info.get(at + 32..at + 96).ok_or("the section map is damaged")?;
        let name: String = name_bytes.iter().take_while(|b| **b != 0).map(|&b| b as char).collect();
        at += 96;
        let mut pg = vec![];
        for _ in 0..n.min(1 << 16) {
            let number = rl(&info, at).ok_or("the section map is damaged")? as i32;
            let start = rll(&info, at + 8).unwrap_or(0);
            at += 16;
            pg.push((number, start));
        }
        descs.push(SectionDesc { name, size, max_decomp, compressed, encrypted, pages: pg });
    }

    let section = |name: &str| -> Result<Vec<u8>, String> {
        let d = descs.iter().find(|d| d.name == name).ok_or_else(|| format!("the drawing has no {name} section"))?;
        if d.encrypted == 1 {
            return Err("the drawing is encrypted".into());
        }
        let total = (d.pages.len() * d.max_decomp).max(d.size as usize);
        if total > 1 << 30 {
            return Err(format!("the {name} section is implausibly large"));
        }
        let mut buf = vec![0u8; total];
        for (number, _) in &d.pages {
            let Some(addr) = page_at(*number) else { continue };
            let head = file.get(addr..addr + 32).ok_or("a section page lies outside the file")?;
            let mask = 0x4164536bu32 ^ addr as u32;
            let f = |k: usize| u32::from_le_bytes(head[k * 4..k * 4 + 4].try_into().unwrap()) ^ mask;
            if f(0) != 0x4163043b {
                return Err(format!("a page of the {name} section is damaged"));
            }
            let data_size = f(2) as usize;
            let start = f(4) as usize;
            let data = file.get(addr + 32..addr + 32 + data_size).ok_or("a section page lies outside the file")?;
            let chunk = if d.compressed { decompress(data, d.max_decomp)? } else { data.to_vec() };
            let end = (start + chunk.len()).min(buf.len());
            if start < end {
                buf[start..end].copy_from_slice(&chunk[..end - start]);
            }
        }
        buf.truncate(d.size as usize);
        Ok(buf)
    };
    let objects = section("AcDb:AcDbObjects")?;
    let handles = section("AcDb:Handles")?;
    let header = section("AcDb:Header").unwrap_or_default();
    let classes = section("AcDb:Classes").unwrap_or_default();
    Ok(Container { ver, maint, map: object_map(&handles), objects, header, classes })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lz77_literals_and_copies() {
        // Literal run "abc" (op 0x00 + len: (0 → 0xf + byte) … use the short
        // form: op 0x01 means 1+3 = 4 literal bytes), then copy 4 bytes from
        // offset 4 (op 0x50: count 5-1 = 4, offset ((0x50>>2)&3 | b2<<2)+1),
        // then end (0x11).
        let src = [0x01, b'a', b'b', b'c', b'd', 0x50, 0x00, 0x11];
        // op 0x50: count = 4, op2 = 0x00: offset = ((0x50 >> 2) & 3 | 0) + 1 = 1.
        let out = decompress(&src, 8).unwrap();
        assert_eq!(&out, b"abcddddd");
        assert_eq!(decrypt_header(&decrypt_header(b"AcFssFcAJMB\0")), b"AcFssFcAJMB\0".to_vec());
    }
}
