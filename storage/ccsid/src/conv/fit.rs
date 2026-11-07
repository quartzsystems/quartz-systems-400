// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Text cut to a record or field length. The one rule every record
//! writer follows: a record of a mixed CCSID holds whole characters with
//! balanced shifts. A cut never falls inside a pair; a record left in
//! double-byte state ends with an SI that fits within the length (the
//! cut moves a pair earlier when it must); a record that continues
//! double-byte text starts with SO; padding is single-byte blanks after
//! the SI. A pure double-byte CCSID (300) is cut between pairs and
//! padded with double-byte spaces. Any other CCSID is cut at the length
//! and padded with the caller's byte.

use super::mixed::{SI, SO, Unit, unit};
use super::{Codec, Form, Shift};

/// Bytes cut to a length.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fitted {
    /// The bytes kept (padded by [`fit`], not by [`cut`]).
    pub bytes: Vec<u8>,
    /// Whether characters were left out.
    pub cut: bool,
}

/// How a CCSID's text is cut.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    /// Anywhere.
    Bytes,
    /// Between pairs.
    Pairs,
    /// Mixed: whole characters, balanced shifts.
    Mixed,
}

fn kind(ccsid: u16) -> Kind {
    match Codec::of(ccsid) {
        Ok(Some(Codec {
            form: Form::Mixed { .. },
            ..
        })) => Kind::Mixed,
        Ok(Some(Codec {
            form: Form::Dbcs { .. },
            ..
        })) => Kind::Pairs,
        _ => Kind::Bytes,
    }
}

/// `bytes` (text in `ccsid`) cut to at most `max` bytes, unpadded: for
/// a variable-length field.
#[must_use]
pub fn cut(ccsid: u16, bytes: &[u8], max: usize) -> Fitted {
    let k = kind(ccsid);
    let mut shift = Shift::Single;
    let mut out = Vec::with_capacity(max.min(bytes.len() + 1));
    if k == Kind::Mixed && bytes.len() <= max && balanced(bytes) {
        out.extend_from_slice(bytes);
        return Fitted {
            bytes: out,
            cut: false,
        };
    }
    let used = take(k, bytes, &mut shift, max, &mut out);
    let cut = has_chars(k, &bytes[used..], shift);
    Fitted { bytes: out, cut }
}

/// One record of `len` from `bytes` (text in `ccsid`): cut as [`cut`]
/// does, then padded with `pad` (double-byte spaces for a pure
/// double-byte CCSID).
#[must_use]
pub fn fit(ccsid: u16, bytes: &[u8], len: usize, pad: u8) -> Fitted {
    let mut f = cut(ccsid, bytes, len);
    f.bytes.resize(len, pad_of(ccsid, pad));
    f
}

/// Records of `len` from one run of text (a stream copied with no line
/// ends): each record balanced, the next starting with SO when the text
/// continues in double-byte state. A double-byte character that cannot
/// fit even alone (a mixed record under four bytes) is left out.
#[must_use]
pub fn wrap(ccsid: u16, bytes: &[u8], len: usize, pad: u8) -> Vec<Vec<u8>> {
    let k = kind(ccsid);
    let pad = pad_of(ccsid, pad);
    let mut shift = Shift::Single;
    let mut at = 0;
    let mut out = Vec::new();
    while has_chars(k, &bytes[at..], shift) {
        let mut r = Vec::with_capacity(len);
        at += take(k, &bytes[at..], &mut shift, len, &mut r);
        if r.is_empty() {
            at += skip(k, &bytes[at..], &mut shift);
            continue;
        }
        r.resize(len, pad);
        out.push(r);
    }
    out
}

/// The pad of a CCSID's records: a pure double-byte one's is 0x4040.
fn pad_of(ccsid: u16, pad: u8) -> u8 {
    if kind(ccsid) == Kind::Pairs {
        0x40
    } else {
        pad
    }
}

/// Whether mixed `bytes` end in single-byte state with no half pair.
fn balanced(bytes: &[u8]) -> bool {
    let mut shift = Shift::Single;
    let mut at = 0;
    while at < bytes.len() {
        match unit(&bytes[at..], shift) {
            Some((u, n)) => {
                if let Unit::Shift(s) = u {
                    shift = s;
                }
                at += n;
            }
            None => return false,
        }
    }
    shift == Shift::Single
}

/// The unit at the start of mixed `bytes`; a half pair at the end is a
/// lone byte.
fn next(bytes: &[u8], shift: Shift) -> Option<(Unit, usize)> {
    if bytes.is_empty() {
        return None;
    }
    Some(unit(bytes, shift).unwrap_or((Unit::Lone, bytes.len())))
}

/// Whether `bytes` hold a character (not only shifts).
fn has_chars(k: Kind, bytes: &[u8], mut shift: Shift) -> bool {
    if k != Kind::Mixed {
        return !bytes.is_empty();
    }
    let mut at = 0;
    while let Some((u, n)) = next(&bytes[at..], shift) {
        match u {
            Unit::Shift(s) => shift = s,
            _ => return true,
        }
        at += n;
    }
    false
}

/// Append to `out` (at most `len` bytes, balanced) what fits from
/// `bytes`, whose input state is `shift`; returns the bytes used and
/// leaves `shift` at the input state after them.
fn take(k: Kind, bytes: &[u8], shift: &mut Shift, len: usize, out: &mut Vec<u8>) -> usize {
    match k {
        Kind::Bytes => {
            let n = bytes.len().min(len);
            out.extend_from_slice(&bytes[..n]);
            n
        }
        Kind::Pairs => {
            // All of it when it fits (an odd last byte too), else whole pairs.
            let n = if bytes.len() <= len {
                bytes.len()
            } else {
                len & !1
            };
            out.extend_from_slice(&bytes[..n]);
            n
        }
        Kind::Mixed => take_mixed(bytes, shift, len, out),
    }
}

fn take_mixed(bytes: &[u8], input: &mut Shift, len: usize, out: &mut Vec<u8>) -> usize {
    let mut state = Shift::Single;
    let mut at = 0;
    while let Some((u, n)) = next(&bytes[at..], *input) {
        // Cost: the shift into the state the unit needs, the unit, and
        // the SI a double-byte unit owes.
        let (need, size) = match u {
            Unit::Shift(s) => {
                *input = s;
                at += n;
                continue;
            }
            Unit::Single(_) => (Shift::Single, 1),
            Unit::Pair(_) => (Shift::Double, 2),
            Unit::Lone => (Shift::Double, n),
        };
        let shift_cost = usize::from(state != need);
        let si = usize::from(need == Shift::Double);
        if out.len() + shift_cost + size + si > len {
            break;
        }
        if state != need {
            out.push(if need == Shift::Double { SO } else { SI });
            state = need;
        }
        out.extend_from_slice(&bytes[at..at + n]);
        at += n;
    }
    if state == Shift::Double {
        out.push(SI);
    }
    at
}

/// Skip the next character that fits nowhere; returns the bytes.
fn skip(k: Kind, bytes: &[u8], shift: &mut Shift) -> usize {
    match k {
        Kind::Bytes => 1.min(bytes.len()),
        Kind::Pairs => 2.min(bytes.len()),
        Kind::Mixed => {
            let mut at = 0;
            while let Some((u, n)) = next(&bytes[at..], *shift) {
                at += n;
                match u {
                    Unit::Shift(s) => *shift = s,
                    _ => break,
                }
            }
            at
        }
    }
}
