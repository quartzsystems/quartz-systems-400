// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Inflating raw deflate data (RFC 1951): stored, fixed and dynamic
//! blocks, canonical Huffman codes decoded a bit at a time from their
//! counts per length.

/// Why deflate data cannot be inflated.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    /// The data ends inside a block.
    #[error("The compressed data ends too soon.")]
    Truncated,
    /// A block type, code or distance that cannot be.
    #[error("The compressed data is damaged.")]
    Damaged,
}

/// Length base values for codes 257..285.
pub const LEN_BASE: [u16; 29] = [
    3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131,
    163, 195, 227, 258,
];
/// Extra bits of each length code.
pub const LEN_EXTRA: [u8; 29] = [
    0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0,
];
/// Distance base values.
pub const DIST_BASE: [u16; 30] = [
    1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537,
    2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577,
];
/// Extra bits of each distance code.
pub const DIST_EXTRA: [u8; 30] = [
    0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13,
    13,
];
/// The order code length code lengths are sent in.
pub const CL_ORDER: [usize; 19] = [16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15];

struct Bits<'a> {
    data: &'a [u8],
    pos: usize,
    buf: u32,
    cnt: u32,
}

impl Bits<'_> {
    fn bits(&mut self, n: u32) -> Result<u32, Error> {
        while self.cnt < n {
            let b = *self.data.get(self.pos).ok_or(Error::Truncated)?;
            self.pos += 1;
            self.buf |= u32::from(b) << self.cnt;
            self.cnt += 8;
        }
        let v = if n == 0 { 0 } else { self.buf & ((1u32 << n) - 1) };
        self.buf = if n >= 32 { 0 } else { self.buf >> n };
        self.cnt -= n;
        Ok(v)
    }

    fn align(&mut self) {
        self.buf = 0;
        self.cnt = 0;
    }
}

struct Huffman {
    counts: [u16; 16],
    symbols: Vec<u16>,
}

impl Huffman {
    /// Codes from lengths; an over-full set is damaged, an incomplete
    /// one is allowed (a single code, unused codes).
    fn new(lengths: &[u8]) -> Result<Self, Error> {
        let mut counts = [0u16; 16];
        for &l in lengths {
            counts[l as usize] += 1;
        }
        counts[0] = 0;
        let mut left: i32 = 1;
        for &c in &counts[1..] {
            left = (left << 1) - i32::from(c);
            if left < 0 {
                return Err(Error::Damaged);
            }
        }
        let mut offs = [0u16; 16];
        for i in 1..15 {
            offs[i + 1] = offs[i] + counts[i];
        }
        let mut symbols = vec![0u16; lengths.len()];
        for (s, &l) in lengths.iter().enumerate() {
            if l != 0 {
                symbols[offs[l as usize] as usize] = s as u16;
                offs[l as usize] += 1;
            }
        }
        Ok(Self { counts, symbols })
    }

    fn decode(&self, b: &mut Bits<'_>) -> Result<u16, Error> {
        let mut code: i32 = 0;
        let mut first: i32 = 0;
        let mut index: i32 = 0;
        for len in 1..16 {
            code |= b.bits(1)? as i32;
            let count = i32::from(self.counts[len]);
            if code - first < count {
                return Ok(self.symbols[(index + code - first) as usize]);
            }
            index += count;
            first += count;
            first <<= 1;
            code <<= 1;
        }
        Err(Error::Damaged)
    }
}

fn fixed() -> (Huffman, Huffman) {
    let mut l = [0u8; 288];
    l[..144].fill(8);
    l[144..256].fill(9);
    l[256..280].fill(7);
    l[280..].fill(8);
    let d = [5u8; 30];
    (
        Huffman::new(&l).expect("fixed codes"),
        Huffman::new(&d).expect("fixed codes"),
    )
}

fn codes(b: &mut Bits<'_>, out: &mut Vec<u8>, lit: &Huffman, dist: &Huffman) -> Result<(), Error> {
    loop {
        let s = lit.decode(b)?;
        match s {
            0..=255 => out.push(s as u8),
            256 => return Ok(()),
            257..=285 => {
                let i = usize::from(s - 257);
                let len = usize::from(LEN_BASE[i]) + b.bits(u32::from(LEN_EXTRA[i]))? as usize;
                let d = usize::from(dist.decode(b)?);
                if d >= 30 {
                    return Err(Error::Damaged);
                }
                let dist = usize::from(DIST_BASE[d]) + b.bits(u32::from(DIST_EXTRA[d]))? as usize;
                if dist > out.len() {
                    return Err(Error::Damaged);
                }
                let start = out.len() - dist;
                for k in 0..len {
                    let c = out[start + k];
                    out.push(c);
                }
            }
            _ => return Err(Error::Damaged),
        }
    }
}

fn dynamic(b: &mut Bits<'_>) -> Result<(Huffman, Huffman), Error> {
    let nlen = b.bits(5)? as usize + 257;
    let ndist = b.bits(5)? as usize + 1;
    let ncode = b.bits(4)? as usize + 4;
    if nlen > 286 || ndist > 30 {
        return Err(Error::Damaged);
    }
    let mut cl = [0u8; 19];
    for &i in &CL_ORDER[..ncode] {
        cl[i] = b.bits(3)? as u8;
    }
    let clh = Huffman::new(&cl)?;
    let mut lengths = vec![0u8; nlen + ndist];
    let mut i = 0;
    while i < nlen + ndist {
        let s = clh.decode(b)?;
        let (val, rep) = match s {
            0..=15 => (s as u8, 1),
            16 => {
                if i == 0 {
                    return Err(Error::Damaged);
                }
                (lengths[i - 1], 3 + b.bits(2)? as usize)
            }
            17 => (0, 3 + b.bits(3)? as usize),
            _ => (0, 11 + b.bits(7)? as usize),
        };
        if i + rep > nlen + ndist {
            return Err(Error::Damaged);
        }
        lengths[i..i + rep].fill(val);
        i += rep;
    }
    if lengths[256] == 0 {
        return Err(Error::Damaged);
    }
    Ok((
        Huffman::new(&lengths[..nlen])?,
        Huffman::new(&lengths[nlen..])?,
    ))
}

/// Inflate raw deflate data.
pub fn inflate(data: &[u8]) -> Result<Vec<u8>, Error> {
    let mut b = Bits {
        data,
        pos: 0,
        buf: 0,
        cnt: 0,
    };
    let mut out = Vec::new();
    loop {
        let last = b.bits(1)? == 1;
        match b.bits(2)? {
            0 => {
                b.align();
                let p = b.pos;
                if p + 4 > data.len() {
                    return Err(Error::Truncated);
                }
                let len = usize::from(u16::from_le_bytes([data[p], data[p + 1]]));
                let nlen = u16::from_le_bytes([data[p + 2], data[p + 3]]);
                if nlen != !(len as u16) {
                    return Err(Error::Damaged);
                }
                if p + 4 + len > data.len() {
                    return Err(Error::Truncated);
                }
                out.extend_from_slice(&data[p + 4..p + 4 + len]);
                b.pos = p + 4 + len;
            }
            1 => {
                let (l, d) = fixed();
                codes(&mut b, &mut out, &l, &d)?;
            }
            2 => {
                let (l, d) = dynamic(&mut b)?;
                codes(&mut b, &mut out, &l, &d)?;
            }
            _ => return Err(Error::Damaged),
        }
        if last {
            return Ok(out);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_streams() {
        // A fixed-code block of "a" and a stored block of "hi".
        assert_eq!(inflate(&[0x4b, 0x04, 0x00]).unwrap(), b"a");
        assert_eq!(inflate(&[0x01, 0x02, 0x00, 0xfd, 0xff, b'h', b'i']).unwrap(), b"hi");
        assert_eq!(inflate(&[0x07]), Err(Error::Damaged));
        assert_eq!(inflate(&[]), Err(Error::Truncated));
    }
}
