// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! CP037 fields: blank padded, cut at their size.

/// `s` as a CP037 field of `n` bytes; characters CP037 lacks become
/// blanks.
#[must_use]
pub fn fixed(s: &str, n: usize) -> Vec<u8> {
    let mut b: Vec<u8> = s
        .chars()
        .map(|c| ccsid::byte_of(c).unwrap_or(0x40))
        .collect();
    b.resize(n, 0x40);
    b.truncate(n);
    b
}

/// `s` in CP037, as long as it is.
#[must_use]
pub fn ebcdic(s: &str) -> Vec<u8> {
    s.chars()
        .map(|c| ccsid::byte_of(c).unwrap_or(0x40))
        .collect()
}

/// A CP037 field back as text, trailing blanks trimmed.
#[must_use]
pub fn field(b: &[u8]) -> String {
    ccsid::decode_trimmed(b)
}

/// A big-endian `i32` at `off`; zero past the end.
#[must_use]
pub fn bin4(b: &[u8], off: usize) -> i32 {
    b.get(off..off + 4)
        .map_or(0, |s| i32::from_be_bytes([s[0], s[1], s[2], s[3]]))
}

/// A little-endian `u32` at `off`; zero past the end.
#[must_use]
pub fn le4(b: &[u8], off: usize) -> u32 {
    b.get(off..off + 4)
        .map_or(0, |s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
}

/// Whether `name` matches `pattern`: `*ALL`, a generic `ABC*` or the
/// name itself, case-insensitively.
#[must_use]
pub fn matches(pattern: &str, name: &str) -> bool {
    let p = pattern.trim().to_ascii_uppercase();
    let n = name.trim().to_ascii_uppercase();
    if p == "*ALL" || p.is_empty() {
        return true;
    }
    match p.strip_suffix('*') {
        Some(prefix) => n.starts_with(prefix),
        None => n == p,
    }
}
