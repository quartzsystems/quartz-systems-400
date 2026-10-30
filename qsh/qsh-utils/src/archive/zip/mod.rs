// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The zip container (jar files): local headers, the central directory
//! and its end record, stored or deflated entries, MS-DOS times and
//! Unix modes in the external attributes.

pub mod crc;
pub mod deflate;
pub mod inflate;

use crate::common::time::{Zone, to_epoch};

/// Entry methods.
pub const STORED: u16 = 0;
/// Deflated.
pub const DEFLATED: u16 = 8;

const LOCAL: u32 = 0x0403_4b50;
const CENTRAL: u32 = 0x0201_4b50;
const END: u32 = 0x0605_4b50;

/// One entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    /// The name (`dir/` for a directory).
    pub name: String,
    /// `STORED` or `DEFLATED`.
    pub method: u16,
    /// CRC-32 of the uncompressed data.
    pub crc: u32,
    /// Uncompressed size.
    pub size: u64,
    /// The data as stored.
    pub data: Vec<u8>,
    /// MS-DOS time and date.
    pub dos: (u16, u16),
    /// Unix mode bits, when the entry carries them.
    pub mode: Option<u32>,
}

impl Entry {
    /// An entry for bytes, deflated when that is smaller.
    pub fn new(name: &str, plain: &[u8], deflate: bool, dos: (u16, u16), mode: Option<u32>) -> Self {
        let crc = crc::crc32(plain);
        let (method, data) = if deflate && !plain.is_empty() {
            let z = deflate::deflate(plain);
            if z.len() < plain.len() {
                (DEFLATED, z)
            } else {
                (STORED, plain.to_vec())
            }
        } else {
            (STORED, plain.to_vec())
        };
        Self {
            name: name.to_string(),
            method,
            crc,
            size: plain.len() as u64,
            data,
            dos,
            mode,
        }
    }

    /// Is it a directory?
    pub fn is_dir(&self) -> bool {
        self.name.ends_with('/')
    }

    /// The uncompressed data, checked against the CRC.
    pub fn plain(&self) -> Result<Vec<u8>, String> {
        let d = match self.method {
            STORED => self.data.clone(),
            DEFLATED => inflate::inflate(&self.data).map_err(|e| e.to_string())?,
            m => return Err(format!("The compression method {m} is not supported.")),
        };
        if crc::crc32(&d) != self.crc || d.len() as u64 != self.size {
            return Err("The data does not match its check value.".to_string());
        }
        Ok(d)
    }
}

/// MS-DOS time and date of a time in a zone (1980 at the earliest).
pub fn dos_time(epoch: i64, zone: &Zone) -> (u16, u16) {
    let t = zone.local(epoch);
    if t.year < 1980 {
        return (0, 0x21);
    }
    let time = (t.hour << 11 | t.min << 5 | (t.sec / 2)) as u16;
    let date = (((t.year - 1980) as u32) << 9 | t.mon << 5 | t.mday) as u16;
    (time, date)
}

/// The time of an MS-DOS time and date, read as the zone's local time.
pub fn from_dos(dos: (u16, u16), zone: &Zone) -> i64 {
    let (t, d) = (u32::from(dos.0), u32::from(dos.1));
    let year = i64::from(d >> 9) + 1980;
    let mon = (d >> 5 & 15).clamp(1, 12);
    let day = (d & 31).max(1);
    let local = to_epoch(year, mon, day, t >> 11, t >> 5 & 63, (t & 31) * 2);
    // The offset in force near that time.
    let off = zone.local(local).gmtoff;
    local - off
}

fn u16le(b: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes([*b.get(at)?, *b.get(at + 1)?]))
}

fn u32le(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(at..at + 4)?.try_into().ok()?))
}

fn put16(o: &mut Vec<u8>, v: u16) {
    o.extend_from_slice(&v.to_le_bytes());
}

fn put32(o: &mut Vec<u8>, v: u32) {
    o.extend_from_slice(&v.to_le_bytes());
}

/// Write an archive.
pub fn write(entries: &[Entry]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut central = Vec::new();
    for e in entries {
        let offset = out.len() as u32;
        let need = if e.method == DEFLATED || e.is_dir() { 20 } else { 10 };
        let flags: u16 = if e.name.is_ascii() { 0 } else { 0x0800 };
        let header = |o: &mut Vec<u8>| {
            put16(o, flags);
            put16(o, e.method);
            put16(o, e.dos.0);
            put16(o, e.dos.1);
            put32(o, e.crc);
            put32(o, e.data.len() as u32);
            put32(o, e.size as u32);
            put16(o, e.name.len() as u16);
            put16(o, 0);
        };
        put32(&mut out, LOCAL);
        put16(&mut out, need);
        header(&mut out);
        out.extend_from_slice(e.name.as_bytes());
        out.extend_from_slice(&e.data);
        put32(&mut central, CENTRAL);
        put16(&mut central, if e.mode.is_some() { 0x0314 } else { 0x0014 });
        put16(&mut central, need);
        header(&mut central);
        put16(&mut central, 0);
        put16(&mut central, 0);
        put16(&mut central, 0);
        let ext = e.mode.map_or(0, |m| m << 16) | if e.is_dir() { 0x10 } else { 0 };
        put32(&mut central, ext);
        put32(&mut central, offset);
        central.extend_from_slice(e.name.as_bytes());
    }
    let cd_at = out.len() as u32;
    let cd_len = central.len() as u32;
    out.extend(central);
    put32(&mut out, END);
    put16(&mut out, 0);
    put16(&mut out, 0);
    put16(&mut out, entries.len() as u16);
    put16(&mut out, entries.len() as u16);
    put32(&mut out, cd_len);
    put32(&mut out, cd_at);
    put16(&mut out, 0);
    out
}

/// Why bytes are not a zip archive.
pub const NOT_ZIP: &str = "The input is not a zip or jar archive.";

/// Read an archive through its central directory.
pub fn read(b: &[u8]) -> Result<Vec<Entry>, String> {
    if b.len() < 22 {
        return Err(NOT_ZIP.to_string());
    }
    let low = b.len().saturating_sub(22 + 65535);
    let end = (low..=b.len() - 22)
        .rev()
        .find(|&i| u32le(b, i) == Some(END))
        .ok_or(NOT_ZIP)?;
    let damaged = || "The zip archive is damaged.".to_string();
    let count = u16le(b, end + 10).ok_or_else(damaged)?;
    let mut at = u32le(b, end + 16).ok_or_else(damaged)? as usize;
    let mut out = Vec::new();
    for _ in 0..count {
        if u32le(b, at) != Some(CENTRAL) {
            return Err(damaged());
        }
        let made = u16le(b, at + 4).ok_or_else(damaged)?;
        let method = u16le(b, at + 10).ok_or_else(damaged)?;
        let dos = (
            u16le(b, at + 12).ok_or_else(damaged)?,
            u16le(b, at + 14).ok_or_else(damaged)?,
        );
        let crc = u32le(b, at + 16).ok_or_else(damaged)?;
        let csize = u32le(b, at + 20).ok_or_else(damaged)? as usize;
        let size = u64::from(u32le(b, at + 24).ok_or_else(damaged)?);
        let nlen = usize::from(u16le(b, at + 28).ok_or_else(damaged)?);
        let xlen = usize::from(u16le(b, at + 30).ok_or_else(damaged)?);
        let clen = usize::from(u16le(b, at + 32).ok_or_else(damaged)?);
        let ext = u32le(b, at + 38).ok_or_else(damaged)?;
        let local = u32le(b, at + 42).ok_or_else(damaged)? as usize;
        let name = b.get(at + 46..at + 46 + nlen).ok_or_else(damaged)?;
        let name = String::from_utf8_lossy(name).into_owned();
        at += 46 + nlen + xlen + clen;
        if u32le(b, local) != Some(LOCAL) {
            return Err(damaged());
        }
        let lnlen = usize::from(u16le(b, local + 26).ok_or_else(damaged)?);
        let lxlen = usize::from(u16le(b, local + 28).ok_or_else(damaged)?);
        let data_at = local + 30 + lnlen + lxlen;
        let data = b.get(data_at..data_at + csize).ok_or_else(damaged)?.to_vec();
        let mode = (made >> 8 == 3 && ext >> 16 != 0).then_some((ext >> 16) & 0o7777);
        out.push(Entry {
            name,
            method,
            crc,
            size,
            data,
            dos,
            mode,
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn archive_round_trip() {
        let z = Zone::utc();
        let dos = dos_time(1_780_000_000, &z);
        assert_eq!(from_dos(dos, &z), 1_780_000_000);
        let text = b"hello hello hello hello hello".to_vec();
        let es = vec![
            Entry::new("d/", b"", false, dos, Some(0o755)),
            Entry::new("d/a.txt", &text, true, dos, Some(0o644)),
            Entry::new("b", b"x", true, dos, None),
        ];
        let back = read(&write(&es)).unwrap();
        assert_eq!(back, es);
        assert_eq!(back[1].method, DEFLATED);
        assert_eq!(back[1].plain().unwrap(), text);
        assert!(read(b"not a zip at all, not at all").is_err());
    }
}
