// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Names: the ISO 9660 level 2 identifiers made from a file's real
//! name, Joliet's UCS-2 form of it, and decoding both back.

use crate::error::{Error, Result};

/// Longest level 2 file identifier, name, dot and extension.
pub const MAX_FILE_ID: usize = 30;
/// Longest level 2 directory identifier.
pub const MAX_DIR_ID: usize = 31;
/// Longest Joliet name, in UCS-2 characters.
pub const MAX_JOLIET: usize = 64;
/// Longest primary volume identifier.
pub const MAX_VOLUME_ID: usize = 32;
/// Longest Joliet volume identifier, in UCS-2 characters.
pub const MAX_JOLIET_VOLUME_ID: usize = 16;

/// An ISO 9660 d-character: `A-Z 0-9 _`.
#[must_use]
pub fn is_d_char(b: u8) -> bool {
    b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_'
}

/// Check a real name (one path component).
pub fn check_name(name: &str) -> Result<()> {
    if name.is_empty()
        || name == "."
        || name == ".."
        || name.contains(['/', '\0'])
        || name.chars().any(char::is_control)
    {
        return Err(Error::BadName(name.to_string()));
    }
    Ok(())
}

fn sanitize(s: &str) -> String {
    s.chars()
        .map(|c| {
            let u = c.to_ascii_uppercase();
            if u.is_ascii() && is_d_char(u as u8) {
                u
            } else {
                '_'
            }
        })
        .collect()
}

/// The level 2 identifier for a real name, before de-duplication:
/// upper case, other characters as `_`; files keep one dot (`NAME.EXT`,
/// or `NAME.` with no extension).
#[must_use]
pub fn primary_base(name: &str, dir: bool) -> String {
    if dir {
        let mut s = sanitize(name);
        s.truncate(MAX_DIR_ID);
        return s;
    }
    let (base, ext) = match name.rsplit_once('.') {
        Some((b, e)) if !b.is_empty() => (sanitize(b), sanitize(e)),
        _ => (sanitize(name), String::new()),
    };
    if base.len() + 1 + ext.len() <= MAX_FILE_ID {
        return format!("{base}.{ext}");
    }
    let ext: String = ext.chars().take(10).collect();
    let base: String = base.chars().take(MAX_FILE_ID - 1 - ext.len()).collect();
    format!("{base}.{ext}")
}

/// A variant of `base` carrying `_N`, still within the limits.
#[must_use]
pub fn primary_variant(base: &str, dir: bool, n: u32) -> String {
    let tag = format!("_{n}");
    if dir {
        let keep = MAX_DIR_ID.saturating_sub(tag.len()).min(base.len());
        return format!("{}{tag}", &base[..keep]);
    }
    let (b, e) = base.split_once('.').unwrap_or((base, ""));
    let keep = (MAX_FILE_ID - 1 - e.len())
        .saturating_sub(tag.len())
        .min(b.len());
    format!("{}{tag}.{e}", &b[..keep])
}

/// Check that a name can be a Joliet name.
pub fn check_joliet(name: &str) -> Result<()> {
    let bad = name
        .chars()
        .any(|c| matches!(c, '*' | '/' | ':' | ';' | '?' | '\\') || u32::from(c) > 0xFFFF);
    if bad || name.chars().count() > MAX_JOLIET {
        return Err(Error::BadName(name.to_string()));
    }
    Ok(())
}

/// UCS-2 big-endian bytes.
#[must_use]
pub fn ucs2(s: &str) -> Vec<u8> {
    s.encode_utf16().flat_map(u16::to_be_bytes).collect()
}

/// A space-padded UCS-2 field of `len` bytes.
pub fn ucs2_field(dst: &mut [u8], s: &str) {
    for pair in dst.chunks_mut(2) {
        pair.copy_from_slice(&[0, b' '][..pair.len()]);
    }
    for (i, u) in s.encode_utf16().enumerate() {
        if 2 * i + 2 > dst.len() {
            break;
        }
        dst[2 * i..2 * i + 2].copy_from_slice(&u.to_be_bytes());
    }
}

/// Decode UCS-2 big-endian (lossy).
#[must_use]
pub fn from_ucs2(b: &[u8]) -> String {
    let units: Vec<u16> = b
        .as_chunks::<2>()
        .0
        .iter()
        .map(|p| u16::from_be_bytes(*p))
        .collect();
    String::from_utf16_lossy(&units)
}

/// A recorded identifier without its `;N` version and, for a file with
/// no extension, the trailing dot.
#[must_use]
pub fn strip_version(id: &str) -> String {
    let s = match id.rfind(';') {
        Some(i) if id[i + 1..].bytes().all(|b| b.is_ascii_digit()) => &id[..i],
        _ => id,
    };
    s.strip_suffix('.').unwrap_or(s).to_string()
}

/// The primary volume identifier for a real one: d-characters, 32 at
/// most.
#[must_use]
pub fn primary_volume_id(id: &str) -> String {
    let mut s = sanitize(id);
    s.truncate(MAX_VOLUME_ID);
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn primary_names() {
        assert_eq!(primary_base("EFI.IMG", false), "EFI.IMG");
        assert_eq!(primary_base("qs400.efi", false), "QS400.EFI");
        assert_eq!(primary_base("MEDIA", false), "MEDIA.");
        assert_eq!(primary_base("a.b.c", false), "A_B.C");
        assert_eq!(primary_base("boot", true), "BOOT");
        let long = primary_base("a-very-long-file-name-for-the-volume.archive", false);
        assert!(long.len() <= MAX_FILE_ID);
        assert!(long.ends_with(".ARCHIVE"));
        let v = primary_variant(&long, false, 12);
        assert!(v.len() <= MAX_FILE_ID && v.contains("_12."));
        assert_eq!(primary_variant("BOOT", true, 2), "BOOT_2");
        assert_eq!(strip_version("QS400.TXT;1"), "QS400.TXT");
        assert_eq!(strip_version("MEDIA.;1"), "MEDIA");
        assert!(check_joliet("a:b").is_err());
        assert!(check_joliet(&"x".repeat(65)).is_err());
        assert_eq!(from_ucs2(&ucs2("Grüße")), "Grüße");
    }
}
