// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Rock Ridge (RRIP 1.10 over SUSP 1.10) in the primary tree's records:
//! `SP` and `ER` on the root's `.`, `PX` and `TF` on every record, `NM`
//! with the real name. Reading takes `NM` and skips everything else.

/// `SP`: the SUSP indicator, first in the root's `.` record.
#[must_use]
pub fn sp() -> Vec<u8> {
    vec![b'S', b'P', 7, 1, 0xBE, 0xEF, 0]
}

/// `ER`: names the extension in use.
#[must_use]
#[allow(clippy::cast_possible_truncation)]
pub fn er() -> Vec<u8> {
    const ID: &[u8] = b"RRIP_1991A";
    const DES: &[u8] = b"QS/400 POSIX NAMES";
    const SRC: &[u8] = b"QUARTZ SYSTEMS";
    let mut v = vec![
        b'E',
        b'R',
        0,
        1,
        ID.len() as u8,
        DES.len() as u8,
        SRC.len() as u8,
        1,
    ];
    v.extend_from_slice(ID);
    v.extend_from_slice(DES);
    v.extend_from_slice(SRC);
    v[2] = u8::try_from(v.len()).expect("short");
    v
}

fn both32(v: &mut Vec<u8>, x: u32) {
    v.extend_from_slice(&x.to_le_bytes());
    v.extend_from_slice(&x.to_be_bytes());
}

/// `PX`: mode, links, owner and group (both zero).
#[must_use]
pub fn px(dir: bool) -> Vec<u8> {
    let mut v = vec![b'P', b'X', 36, 1];
    both32(&mut v, if dir { 0o040_555 } else { 0o100_444 });
    both32(&mut v, if dir { 2 } else { 1 });
    both32(&mut v, 0);
    both32(&mut v, 0);
    v
}

/// `TF`: the modification time in the 7-byte form.
#[must_use]
pub fn tf(date: [u8; 7]) -> Vec<u8> {
    let mut v = vec![b'T', b'F', 12, 1, 0x02];
    v.extend_from_slice(&date);
    v
}

/// `NM`: the real name.
#[must_use]
pub fn nm(name: &str) -> Vec<u8> {
    let mut v = vec![b'N', b'M', 0, 1, 0];
    v.extend_from_slice(name.as_bytes());
    v[2] = u8::try_from(v.len()).unwrap_or(255);
    v
}

/// The system use bytes of an entry record.
#[must_use]
pub fn entry(name: &str, dir: bool, date: [u8; 7]) -> Vec<u8> {
    let mut v = px(dir);
    v.extend(nm(name));
    v.extend(tf(date));
    v
}

/// Whether a system use area starts with `SP`.
#[must_use]
pub fn has_sp(su: &[u8]) -> bool {
    su.len() >= 7 && su[0] == b'S' && su[1] == b'P' && su[4] == 0xBE && su[5] == 0xEF
}

/// The `NM` name in a system use area, if any. `CE` continuations are
/// not followed; malformed entries end the scan.
#[must_use]
pub fn name(su: &[u8]) -> Option<String> {
    let mut at = 0usize;
    let mut out: Option<Vec<u8>> = None;
    while at + 4 <= su.len() {
        let len = usize::from(su[at + 2]);
        if len < 4 || at + len > su.len() {
            break;
        }
        let e = &su[at..at + len];
        match (e[0], e[1]) {
            (b'N', b'M') if len >= 5 => {
                let flags = e[4];
                // CURRENT and PARENT name `.` and `..`.
                if flags & 0x06 == 0 {
                    out.get_or_insert_with(Vec::new).extend_from_slice(&e[5..]);
                }
                if flags & 0x01 == 0 && out.is_some() {
                    break;
                }
            }
            (b'S', b'T') => break,
            _ => {}
        }
        at += len;
    }
    out.map(|b| String::from_utf8_lossy(&b).into_owned())
        .filter(|s| !s.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entries_parse_back() {
        let su = entry("qs400.efi", false, [126, 1, 1, 0, 0, 0, 0]);
        assert_eq!(name(&su).as_deref(), Some("qs400.efi"));
        assert!(has_sp(&sp()));
        assert_eq!(er()[2] as usize, er().len());
        assert_eq!(name(&[b'N', b'M', 2, 1]), None);
        assert_eq!(name(&[b'N', b'M', 200, 1, 0, b'x']), None);
    }
}
