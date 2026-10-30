// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! cpio formats: POSIX's portable character header (`070707`, octal
//! fields) and the SVR4 ones (`070701`, hexadecimal fields, 4-byte
//! alignment; `070702` adds a data checksum). Each ends with a member
//! named `TRAILER!!!`.

use super::member::{Kind, Member, kind_of_bits, type_bits};

/// The trailer's name.
pub const TRAILER: &str = "TRAILER!!!";

/// Which cpio header.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cpio {
    /// `070707`.
    Odc,
    /// `070701`.
    Newc,
    /// `070702`.
    Crc,
}

fn oct(out: &mut Vec<u8>, v: u64, w: usize) {
    let s = format!("{v:0w$o}");
    out.extend_from_slice(&s.as_bytes()[s.len() - w..]);
}

fn hex(out: &mut Vec<u8>, v: u64) {
    out.extend_from_slice(format!("{:08X}", v & 0xffff_ffff).as_bytes());
}

fn pad4(out: &mut Vec<u8>) {
    out.resize(out.len().div_ceil(4) * 4, 0);
}

/// Encode a member.
pub fn encode(m: &Member, f: Cpio) -> Vec<u8> {
    let data: &[u8] = match m.kind {
        Kind::File | Kind::Hardlink => &m.data,
        Kind::Symlink => m.link.as_bytes(),
        _ => &[],
    };
    let mode = u64::from(type_bits(m.kind) | (m.mode & 0o7777));
    let mut name = m.name.as_bytes().to_vec();
    name.push(0);
    let mut out = Vec::with_capacity(110 + name.len() + data.len());
    match f {
        Cpio::Odc => {
            out.extend_from_slice(b"070707");
            oct(&mut out, m.dev, 6);
            oct(&mut out, m.ino, 6);
            oct(&mut out, mode, 6);
            oct(&mut out, u64::from(m.uid), 6);
            oct(&mut out, u64::from(m.gid), 6);
            oct(&mut out, u64::from(m.nlink), 6);
            oct(&mut out, u64::from(m.rdev.0 << 8 | m.rdev.1), 6);
            oct(&mut out, m.mtime.max(0) as u64, 11);
            oct(&mut out, name.len() as u64, 6);
            oct(&mut out, data.len() as u64, 11);
            out.extend_from_slice(&name);
            out.extend_from_slice(data);
        }
        Cpio::Newc | Cpio::Crc => {
            out.extend_from_slice(if f == Cpio::Newc { b"070701" } else { b"070702" });
            hex(&mut out, m.ino);
            hex(&mut out, mode);
            hex(&mut out, u64::from(m.uid));
            hex(&mut out, u64::from(m.gid));
            hex(&mut out, u64::from(m.nlink));
            hex(&mut out, m.mtime.max(0) as u64);
            hex(&mut out, data.len() as u64);
            hex(&mut out, m.dev >> 8);
            hex(&mut out, m.dev & 0xff);
            hex(&mut out, u64::from(m.rdev.0));
            hex(&mut out, u64::from(m.rdev.1));
            hex(&mut out, name.len() as u64);
            let check: u64 = if f == Cpio::Crc {
                data.iter().map(|&b| u64::from(b)).sum()
            } else {
                0
            };
            hex(&mut out, check);
            out.extend_from_slice(&name);
            pad4(&mut out);
            out.extend_from_slice(data);
            pad4(&mut out);
        }
    }
    out
}

/// The trailer member.
pub fn trailer(f: Cpio) -> Vec<u8> {
    let mut m = Member::new(TRAILER, Kind::File);
    m.mode = 0;
    encode(&m, f)
}

fn num(b: &[u8], radix: u32) -> Option<u64> {
    let s = std::str::from_utf8(b).ok()?;
    u64::from_str_radix(s, radix).ok()
}

/// The format of a header's magic.
pub fn magic(b: &[u8]) -> Option<Cpio> {
    match b.get(..6)? {
        b"070707" => Some(Cpio::Odc),
        b"070701" => Some(Cpio::Newc),
        b"070702" => Some(Cpio::Crc),
        _ => None,
    }
}

const DAMAGED: &str = "The archive has a damaged header.";

/// Decode every member up to the trailer.
pub fn decode(data: &[u8]) -> Result<Vec<Member>, String> {
    let mut out = Vec::new();
    let mut pos = 0;
    let mut seen: Vec<((u64, u64), String)> = Vec::new();
    loop {
        let rest = &data[pos..];
        let Some(f) = magic(rest) else {
            return Err(DAMAGED.to_string());
        };
        let (mut m, namesize, filesize, hlen) = match f {
            Cpio::Odc => {
                if rest.len() < 76 {
                    return Err(DAMAGED.to_string());
                }
                let n = |a: usize, w: usize| num(&rest[a..a + w], 8).ok_or(DAMAGED);
                let mode = n(18, 6)? as u32;
                let kind = kind_of_bits(mode).ok_or(DAMAGED)?;
                let mut m = Member::new("", kind);
                m.dev = n(6, 6)?;
                m.ino = n(12, 6)?;
                m.mode = mode & 0o7777;
                m.uid = n(24, 6)? as u32;
                m.gid = n(30, 6)? as u32;
                m.nlink = n(36, 6)? as u32;
                let rdev = n(42, 6)? as u32;
                m.rdev = (rdev >> 8, rdev & 0xff);
                m.mtime = n(48, 11)? as i64;
                (m, n(59, 6)? as usize, n(65, 11)? as usize, 76)
            }
            Cpio::Newc | Cpio::Crc => {
                if rest.len() < 110 {
                    return Err(DAMAGED.to_string());
                }
                let n = |i: usize| num(&rest[6 + i * 8..14 + i * 8], 16).ok_or(DAMAGED);
                let mode = n(1)? as u32;
                let kind = kind_of_bits(mode).ok_or(DAMAGED)?;
                let mut m = Member::new("", kind);
                m.ino = n(0)?;
                m.mode = mode & 0o7777;
                m.uid = n(2)? as u32;
                m.gid = n(3)? as u32;
                m.nlink = n(4)? as u32;
                m.mtime = n(5)? as i64;
                m.dev = n(7)? << 8 | n(8)?;
                m.rdev = (n(9)? as u32, n(10)? as u32);
                (m, n(11)? as usize, n(6)? as usize, 110)
            }
        };
        let name_end = hlen + namesize;
        if namesize == 0 || rest.len() < name_end {
            return Err(DAMAGED.to_string());
        }
        let raw = &rest[hlen..name_end - 1];
        m.name = String::from_utf8_lossy(raw).into_owned();
        let data_at = if f == Cpio::Odc {
            name_end
        } else {
            name_end.div_ceil(4) * 4
        };
        if rest.len() < data_at + filesize {
            return Err("The archive ends inside a member.".to_string());
        }
        let body = rest[data_at..data_at + filesize].to_vec();
        let next = if f == Cpio::Odc {
            data_at + filesize
        } else {
            (data_at + filesize).div_ceil(4) * 4
        };
        pos += next.min(rest.len());
        if m.name == TRAILER {
            return Ok(out);
        }
        match m.kind {
            Kind::Symlink => m.link = String::from_utf8_lossy(&body).into_owned(),
            Kind::File => m.data = body,
            _ => {}
        }
        if m.kind == Kind::File && m.nlink > 1 {
            let key = (m.dev, m.ino);
            if let Some((_, first)) = seen.iter().find(|(k, _)| *k == key) {
                m.link.clone_from(first);
                m.kind = Kind::Hardlink;
            } else {
                seen.push((key, m.name.clone()));
            }
        }
        m.name = m.name.trim_end_matches('/').to_string();
        out.push(m);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips() {
        for f in [Cpio::Odc, Cpio::Newc, Cpio::Crc] {
            let mut a = Member::new("dir/a", Kind::File);
            a.data = b"abc".to_vec();
            a.mode = 0o640;
            let mut l = Member::new("dir/l", Kind::Symlink);
            l.link = "a".to_string();
            let mut bytes = encode(&a, f);
            bytes.extend(encode(&l, f));
            bytes.extend(trailer(f));
            let back = decode(&bytes).unwrap();
            assert_eq!(back.len(), 2);
            assert_eq!(back[0].data, b"abc");
            assert_eq!(back[0].mode, 0o640);
            assert_eq!(back[1].link, "a");
        }
    }
}
