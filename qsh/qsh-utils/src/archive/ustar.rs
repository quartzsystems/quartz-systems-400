// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The POSIX ustar format and the older tar format it extends: 512-byte
//! headers with octal fields, data padded to 512 bytes, two zero blocks
//! at the end.

use super::member::{Kind, Member};

/// A tar block.
pub const BLOCK: usize = 512;

fn octal(field: &mut [u8], v: u64) {
    // Digits, then a NUL; a value too wide keeps its low digits.
    let w = field.len() - 1;
    let s = format!("{v:0w$o}");
    let s = &s[s.len() - w..];
    field[..w].copy_from_slice(s.as_bytes());
    field[w] = 0;
}

fn text(field: &mut [u8], s: &str) {
    let b = s.as_bytes();
    let n = b.len().min(field.len());
    field[..n].copy_from_slice(&b[..n]);
}

/// Split a name into ustar's prefix and name fields.
fn split_name(name: &str) -> Option<(String, String)> {
    if name.len() <= 100 {
        return Some((String::new(), name.to_string()));
    }
    let b = name.as_bytes();
    for (i, &c) in b.iter().enumerate() {
        if c == b'/' && i <= 155 && b.len() - i - 1 <= 100 && i > 0 && i + 1 < b.len() {
            return Some((name[..i].to_string(), name[i + 1..].to_string()));
        }
    }
    None
}

/// Encode a member: its header and padded data. `old` writes the tar
/// format before ustar (no magic, no names, no prefix).
pub fn encode(m: &Member, old: bool) -> Result<Vec<u8>, String> {
    let mut name = m.name.clone();
    if m.kind == Kind::Dir {
        name.push('/');
    }
    let (prefix, short) = if old {
        if name.len() > 100 {
            return Err("The name is too long for the format.".to_string());
        }
        (String::new(), name)
    } else {
        split_name(&name).ok_or("The name is too long for the format.")?
    };
    if m.link.len() > 100 {
        return Err("The link target is too long for the format.".to_string());
    }
    let mut h = [0u8; BLOCK];
    text(&mut h[0..100], &short);
    octal(&mut h[100..108], u64::from(m.mode & 0o7777));
    octal(&mut h[108..116], u64::from(m.uid));
    octal(&mut h[116..124], u64::from(m.gid));
    let data: &[u8] = if m.kind == Kind::File { &m.data } else { &[] };
    octal(&mut h[124..136], data.len() as u64);
    octal(&mut h[136..148], m.mtime.max(0) as u64);
    h[156] = match m.kind {
        Kind::File if old => 0,
        Kind::File => b'0',
        Kind::Hardlink => b'1',
        Kind::Symlink => b'2',
        Kind::Char => b'3',
        Kind::Block => b'4',
        Kind::Dir => b'5',
        Kind::Fifo => b'6',
    };
    if old && !matches!(m.kind, Kind::File | Kind::Hardlink | Kind::Symlink | Kind::Dir) {
        return Err("The format cannot hold a special file.".to_string());
    }
    if matches!(m.kind, Kind::Hardlink | Kind::Symlink) {
        text(&mut h[157..257], &m.link);
    }
    if !old {
        h[257..263].copy_from_slice(b"ustar\0");
        h[263..265].copy_from_slice(b"00");
        text(&mut h[265..297], &m.uname);
        text(&mut h[297..329], &m.gname);
        octal(&mut h[329..337], u64::from(m.rdev.0));
        octal(&mut h[337..345], u64::from(m.rdev.1));
        text(&mut h[345..500], &prefix);
    }
    h[148..156].fill(b' ');
    let sum: u32 = h.iter().map(|&b| u32::from(b)).sum();
    let s = format!("{sum:06o}");
    h[148..154].copy_from_slice(s.as_bytes());
    h[154] = 0;
    h[155] = b' ';
    let mut out = h.to_vec();
    out.extend_from_slice(data);
    out.resize(out.len().div_ceil(BLOCK) * BLOCK, 0);
    Ok(out)
}

fn field_text(f: &[u8]) -> String {
    let end = f.iter().position(|&b| b == 0).unwrap_or(f.len());
    String::from_utf8_lossy(&f[..end]).into_owned()
}

fn field_num(f: &[u8]) -> Option<u64> {
    let s = field_text(f);
    let s = s.trim_matches(|c: char| c == ' ' || c == '\0');
    if s.is_empty() {
        return Some(0);
    }
    u64::from_str_radix(s, 8).ok()
}

/// Is this block a tar header with a good checksum?
pub fn is_header(h: &[u8]) -> bool {
    if h.len() < BLOCK {
        return false;
    }
    let Some(want) = field_num(&h[148..156]) else {
        return false;
    };
    let mut sum: u64 = 0;
    let mut ssum: i64 = 0;
    for (i, &b) in h[..BLOCK].iter().enumerate() {
        let b = if (148..156).contains(&i) { b' ' } else { b };
        sum += u64::from(b);
        ssum += i64::from(b as i8);
    }
    want == sum || i64::try_from(want).is_ok_and(|w| w == ssum)
}

/// Is this a ustar header (not the older format)?
pub fn is_ustar(h: &[u8]) -> bool {
    h.len() >= BLOCK && &h[257..262] == b"ustar"
}

/// Decode every member; stop at a zero block or the end.
pub fn decode(data: &[u8]) -> Result<Vec<Member>, String> {
    let mut out = Vec::new();
    let mut pos = 0;
    while pos + BLOCK <= data.len() {
        let h = &data[pos..pos + BLOCK];
        if h.iter().all(|&b| b == 0) {
            break;
        }
        if !is_header(h) {
            return Err("The archive has a damaged header.".to_string());
        }
        let ustar = is_ustar(h);
        let mut name = field_text(&h[0..100]);
        if ustar {
            let prefix = field_text(&h[345..500]);
            if !prefix.is_empty() {
                name = format!("{prefix}/{name}");
            }
        }
        let num = |r: std::ops::Range<usize>| field_num(&h[r]).ok_or("The archive has a damaged header.");
        let size = num(124..136)? as usize;
        let mut kind = match h[156] {
            b'0' | 0 | b'7' => Kind::File,
            b'1' => Kind::Hardlink,
            b'2' => Kind::Symlink,
            b'3' => Kind::Char,
            b'4' => Kind::Block,
            b'5' => Kind::Dir,
            b'6' => Kind::Fifo,
            _ => Kind::File,
        };
        if name.ends_with('/') {
            kind = Kind::Dir;
        }
        let trimmed = name.trim_end_matches('/');
        let mut m = Member::new(if trimmed.is_empty() { "/" } else { trimmed }, kind);
        m.mode = num(100..108)? as u32 & 0o7777;
        m.uid = num(108..116)? as u32;
        m.gid = num(116..124)? as u32;
        m.mtime = num(136..148)? as i64;
        m.link = field_text(&h[157..257]);
        if ustar {
            m.uname = field_text(&h[265..297]);
            m.gname = field_text(&h[297..329]);
            m.rdev = (num(329..337)? as u32, num(337..345)? as u32);
        }
        pos += BLOCK;
        let has_data = matches!(kind, Kind::File);
        let take = if has_data { size } else { 0 };
        if pos + take > data.len() {
            return Err("The archive ends inside a member.".to_string());
        }
        m.data = data[pos..pos + take].to_vec();
        // A header's size is skipped even when the type carries no data.
        pos += size.div_ceil(BLOCK) * BLOCK;
        out.push(m);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_and_long_names() {
        let mut m = Member::new("a/b.txt", Kind::File);
        m.data = b"hello".to_vec();
        m.mtime = 1_780_000_000;
        m.uname = "QSHUSER".to_string();
        let long = format!("{}/{}", "d".repeat(120), "f".repeat(90));
        let mut l = Member::new(&long, Kind::File);
        l.data = vec![7; 1000];
        let mut bytes = encode(&m, false).unwrap();
        bytes.extend(encode(&l, false).unwrap());
        bytes.extend([0u8; 1024]);
        let back = decode(&bytes).unwrap();
        assert_eq!(back[0].name, "a/b.txt");
        assert_eq!(back[0].data, b"hello");
        assert_eq!(back[0].uname, "QSHUSER");
        assert_eq!(back[1].name, long);
        assert_eq!(back[1].data.len(), 1000);
        assert!(encode(&Member::new(&"x".repeat(300), Kind::File), false).is_err());
    }
}
