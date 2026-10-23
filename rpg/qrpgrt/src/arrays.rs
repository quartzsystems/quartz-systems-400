// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Array searches and sorts: `LOOKUP`, the `%LOOKUP` and `%TLOOKUP`
//! families, `SORTA`. Elements compare by value (numbers) or as bytes
//! blank-padded (character, EBCDIC order).

use std::cmp::Ordering;

use crate::mov::image;
use crate::value::{Desc, bytes, read_num};

/// Compare two values of one type.
pub fn compare(a: u64, b: u64, d: &Desc) -> Ordering {
    if d.numeric() {
        return match (read_num(a, d), read_num(b, d)) {
            (Ok(x), Ok(y)) => x.cmp(&y),
            _ => bytes(a, d.size()).to_vec().cmp(&bytes(b, d.size()).to_vec()),
        };
    }
    let x = image(a, d).unwrap_or_default();
    let y = image(b, d).unwrap_or_default();
    let n = x.len().max(y.len());
    let pad = |v: &[u8]| -> Vec<u8> {
        let mut v = v.to_vec();
        v.resize(n, 0x40);
        v
    };
    pad(&x).cmp(&pad(&y))
}

/// What a search wants: bits 1 higher, 2 lower, 4 equal.
pub const HI: i32 = 1;
/// Lower.
pub const LO: i32 = 2;
/// Equal.
pub const EQ: i32 = 4;

/// Search `count` elements from `start` (0-based) of an array of
/// `total`: the 1-based index found and what matched (`HI`, `LO`,
/// `EQ`), or `None`. A higher or lower match is the nearest such
/// element, the first of equals.
pub fn lookup(arg: u64, arr: u64, d: &Desc, start: usize, count: usize, want: i32, stride: u64) -> Option<(usize, i32)> {
    let size = stride;
    let end = start.saturating_add(count);
    let mut best: Option<(usize, i32)> = None;
    for i in start..end {
        let e = arr + i as u64 * size;
        match compare(e, arg, d) {
            Ordering::Equal if want & EQ != 0 => return Some((i + 1, EQ)),
            Ordering::Greater if want & HI != 0 => {
                let better = match best {
                    None => true,
                    Some((j, _)) => compare(e, arr + (j as u64 - 1) * size, d) == Ordering::Less,
                };
                if better {
                    best = Some((i + 1, HI));
                }
            }
            Ordering::Less if want & LO != 0 => {
                let better = match best {
                    None => true,
                    Some((j, _)) => compare(e, arr + (j as u64 - 1) * size, d) == Ordering::Greater,
                };
                if better {
                    best = Some((i + 1, LO));
                }
            }
            _ => {}
        }
    }
    // An equal element after the nearest unequal one still wins.
    best
}

/// `SORTA`: `count` elements of `elsize` bytes from `base`, by the key
/// at `keyoff` of type `kd`, ascending or descending.
pub fn sorta(base: u64, count: usize, elsize: usize, keyoff: usize, kd: &Desc, descending: bool) {
    if count < 2 {
        return;
    }
    let all = bytes(base, count * elsize).to_vec();
    let mut idx: Vec<usize> = (0..count).collect();
    idx.sort_by(|&i, &j| {
        let a = base + (i * elsize + keyoff) as u64;
        let b = base + (j * elsize + keyoff) as u64;
        let o = compare(a, b, kd);
        if descending { o.reverse() } else { o }
    });
    let out = bytes(base, count * elsize);
    for (k, i) in idx.iter().enumerate() {
        out[k * elsize..(k + 1) * elsize].copy_from_slice(&all[i * elsize..(i + 1) * elsize]);
    }
}

/// `%MAXARR` and `%MINARR`: the 1-based index of the first largest
/// (smallest) of `count` elements from `start`; 0 when there are none.
pub fn extreme(arr: u64, d: &Desc, start: usize, count: usize, stride: u64, max: bool) -> usize {
    let mut best: Option<usize> = None;
    for i in start..start.saturating_add(count) {
        let better = match best {
            None => true,
            Some(j) => {
                let o = compare(arr + i as u64 * stride, arr + j as u64 * stride, d);
                if max { o == Ordering::Greater } else { o == Ordering::Less }
            }
        };
        if better {
            best = Some(i);
        }
    }
    best.map_or(0, |i| i + 1)
}
