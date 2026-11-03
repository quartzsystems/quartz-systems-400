// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Mixed and double-byte EBCDIC. SO enters double-byte state and SI
//! leaves it; in double-byte state each character is a pair. An
//! unmapped pair decodes to U+FFFD; a character with no encoding
//! becomes 0x3F when it is a single-byte one (U+0000 to U+00FF), else
//! the double-byte substitution 0xFEFE. The encoder shifts only when
//! the next character needs the other state.

use std::collections::HashMap;

use super::dbcs::DbcsTable;
use super::stream::Shift;

/// Shift out: double-byte data follows.
pub const SO: u8 = 0x0E;
/// Shift in: single-byte data follows.
pub const SI: u8 = 0x0F;
/// The double-byte space.
pub const DBCS_SPACE: u16 = 0x4040;
/// The double-byte substitution character.
pub const DBCS_SUB: u16 = 0xFEFE;

/// What the bytes at a position are.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Unit {
    /// SO or SI: the new state.
    Shift(Shift),
    /// One single-byte character.
    Single(u8),
    /// One pair.
    Pair(u16),
    /// A lone byte in double-byte state before SO or SI.
    Lone,
}

/// The unit at the start of `bytes` in state `shift` and its length;
/// `None` when it needs more bytes (half a pair).
pub(super) fn unit(bytes: &[u8], shift: Shift) -> Option<(Unit, usize)> {
    let &b = bytes.first()?;
    match (b, shift) {
        (SO, _) => Some((Unit::Shift(Shift::Double), 1)),
        (SI, _) => Some((Unit::Shift(Shift::Single), 1)),
        (_, Shift::Single) => Some((Unit::Single(b), 1)),
        (_, Shift::Double) => match bytes.get(1) {
            None => None,
            Some(&(SO | SI)) => Some((Unit::Lone, 1)),
            Some(&c) => Some((Unit::Pair(u16::from_be_bytes([b, c])), 2)),
        },
    }
}

/// Decode mixed bytes from state `shift`; returns the bytes used.
pub(super) fn decode(
    sbcs: &[u32; 256],
    dbcs: &DbcsTable,
    bytes: &[u8],
    shift: &mut Shift,
    out: &mut Vec<char>,
) -> usize {
    let mut at = 0;
    while let Some((u, n)) = unit(&bytes[at..], *shift) {
        match u {
            Unit::Shift(s) => *shift = s,
            Unit::Single(b) => out.push(char::from_u32(sbcs[usize::from(b)]).unwrap_or('\u{1A}')),
            Unit::Pair(p) => out.push(dbcs.decode(p).unwrap_or('\u{FFFD}')),
            Unit::Lone => out.push('\u{FFFD}'),
        }
        at += n;
    }
    at
}

/// Decode pure double-byte data; an odd last byte is left.
pub(super) fn decode_pairs(dbcs: &DbcsTable, bytes: &[u8], out: &mut Vec<char>) -> usize {
    let whole = bytes.len() & !1;
    out.extend(
        bytes[..whole]
            .as_chunks::<2>()
            .0
            .iter()
            .map(|p| dbcs.decode(u16::from_be_bytes(*p)).unwrap_or('\u{FFFD}')),
    );
    whole
}

/// Encode one character from output state `shift`.
pub(super) fn encode(
    sbcs: &HashMap<u32, u8>,
    dbcs: &DbcsTable,
    ch: char,
    shift: &mut Shift,
    out: &mut Vec<u8>,
) -> bool {
    let to = |want: Shift, shift: &mut Shift, out: &mut Vec<u8>| {
        if *shift != want {
            out.push(if want == Shift::Double { SO } else { SI });
            *shift = want;
        }
    };
    if let Some(&b) = sbcs.get(&u32::from(ch)) {
        to(Shift::Single, shift, out);
        out.push(b);
        return true;
    }
    if let Some(p) = dbcs.encode(ch) {
        to(Shift::Double, shift, out);
        out.extend_from_slice(&p.to_be_bytes());
        return true;
    }
    if u32::from(ch) <= 0xFF {
        to(Shift::Single, shift, out);
        out.push(0x3F);
    } else {
        to(Shift::Double, shift, out);
        out.extend_from_slice(&DBCS_SUB.to_be_bytes());
    }
    false
}
