// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The `.Z` format: a three-byte header (`1f 9d`, then the code size
//! limit with `0x80` for block mode) and LZW codes of 9 bits growing
//! to the limit, packed low bit first in groups of eight codes. A
//! group is padded out when the code size changes, as the format has
//! always done, and code 256 clears the table when the ratio falls.

use std::collections::HashMap;

/// The header's first two bytes.
pub const MAGIC: [u8; 2] = [0x1f, 0x9d];
/// Block mode: code 256 clears the table.
const BLOCK_MODE: u8 = 0x80;
const CLEAR: u32 = 256;
const FIRST: u32 = 257;
const CHECK_GAP: u64 = 10_000;

/// Why a `.Z` stream cannot be decoded.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    /// Not a `.Z` header.
    #[error("The data is not in compressed format.")]
    NotCompressed,
    /// A code size limit above 16.
    #[error("The data was compressed with more than 16 bits.")]
    TooManyBits,
    /// A code that cannot follow.
    #[error("The compressed data is damaged.")]
    Corrupt,
}

fn max_code(bits: u32, max_bits: u32) -> u32 {
    if bits == max_bits {
        1 << max_bits
    } else {
        (1 << bits) - 1
    }
}

struct Packer {
    out: Vec<u8>,
    buf: [u8; 16],
    offset: usize,
    bits: u32,
}

impl Packer {
    fn put(&mut self, code: u32) {
        for i in 0..self.bits as usize {
            if code >> i & 1 != 0 {
                let at = self.offset + i;
                self.buf[at / 8] |= 1 << (at % 8);
            }
        }
        self.offset += self.bits as usize;
        if self.offset == self.bits as usize * 8 {
            self.flush_group();
        }
    }

    fn flush_group(&mut self) {
        if self.offset > 0 {
            self.out.extend_from_slice(&self.buf[..self.bits as usize]);
        }
        self.offset = 0;
        self.buf = [0; 16];
    }

    fn end(&mut self) {
        self.out.extend_from_slice(&self.buf[..self.offset.div_ceil(8)]);
        self.offset = 0;
    }
}

struct Encoder {
    p: Packer,
    max_bits: u32,
    max_code: u32,
    free: u32,
    clear: bool,
}

impl Encoder {
    fn output(&mut self, code: u32) {
        self.p.put(code);
        if self.free > self.max_code || self.clear {
            self.p.flush_group();
            if self.clear {
                self.p.bits = 9;
                self.clear = false;
            } else {
                self.p.bits += 1;
            }
            self.max_code = max_code(self.p.bits, self.max_bits);
        }
    }
}

/// Compress with codes up to `max_bits` (9 to 16).
pub fn compress(data: &[u8], max_bits: u32) -> Vec<u8> {
    let max_bits = max_bits.clamp(9, 16);
    let mut e = Encoder {
        p: Packer {
            out: vec![MAGIC[0], MAGIC[1], BLOCK_MODE | max_bits as u8],
            buf: [0; 16],
            offset: 0,
            bits: 9,
        },
        max_bits,
        max_code: max_code(9, max_bits),
        free: FIRST,
        clear: false,
    };
    let Some((&first, rest)) = data.split_first() else {
        return e.p.out;
    };
    let limit = 1u32 << max_bits;
    let mut dict: HashMap<(u32, u8), u32> = HashMap::new();
    let mut ent = u32::from(first);
    let mut in_count: u64 = 1;
    let mut checkpoint = CHECK_GAP;
    let mut ratio: u64 = 0;
    for &c in rest {
        in_count += 1;
        if let Some(&code) = dict.get(&(ent, c)) {
            ent = code;
            continue;
        }
        e.output(ent);
        let prefix = ent;
        ent = u32::from(c);
        if e.free < limit {
            dict.insert((prefix, c), e.free);
            e.free += 1;
        } else if in_count >= checkpoint {
            checkpoint = in_count + CHECK_GAP;
            let out = (e.p.out.len() as u64).max(1);
            let rat = (in_count << 8) / out;
            if rat > ratio {
                ratio = rat;
            } else {
                ratio = 0;
                dict.clear();
                e.free = FIRST;
                e.clear = true;
                e.output(CLEAR);
            }
        }
    }
    e.output(ent);
    e.p.end();
    e.p.out
}

struct Unpacker<'a> {
    input: &'a [u8],
    pos: usize,
    chunk: [u8; 16],
    offset: usize,
    size: usize,
    bits: u32,
}

impl Unpacker<'_> {
    fn code(&mut self, renew: bool) -> Option<u32> {
        if renew || self.offset >= self.size {
            let n = (self.bits as usize).min(self.input.len() - self.pos);
            if n == 0 {
                return None;
            }
            self.chunk = [0; 16];
            self.chunk[..n].copy_from_slice(&self.input[self.pos..self.pos + n]);
            self.pos += n;
            self.offset = 0;
            self.size = (n * 8).saturating_sub(self.bits as usize - 1);
            if self.size == 0 {
                return None;
            }
        }
        let mut code = 0u32;
        for i in 0..self.bits as usize {
            let at = self.offset + i;
            if self.chunk[at / 8] >> (at % 8) & 1 != 0 {
                code |= 1 << i;
            }
        }
        self.offset += self.bits as usize;
        Some(code)
    }
}

/// Decompress a `.Z` stream.
pub fn decompress(data: &[u8]) -> Result<Vec<u8>, Error> {
    if data.len() < 3 || data[..2] != MAGIC {
        return Err(Error::NotCompressed);
    }
    let max_bits = u32::from(data[2] & 0x1f);
    let block = data[2] & BLOCK_MODE != 0;
    if max_bits > 16 {
        return Err(Error::TooManyBits);
    }
    if max_bits < 9 {
        return Err(Error::Corrupt);
    }
    let limit = 1u32 << max_bits;
    let mut u = Unpacker {
        input: &data[3..],
        pos: 0,
        chunk: [0; 16],
        offset: 0,
        size: 0,
        bits: 9,
    };
    let mut maxc = max_code(9, max_bits);
    let mut free = if block { FIRST } else { 256 };
    let mut prefix = vec![0u32; limit as usize];
    let mut suffix = vec![0u8; limit as usize];
    let mut out = Vec::new();
    let mut clear = false;
    let next = |u: &mut Unpacker<'_>, free: u32, maxc: &mut u32, clear: &mut bool| {
        let mut renew = false;
        if free > *maxc {
            u.bits += 1;
            *maxc = max_code(u.bits, max_bits);
            renew = true;
        }
        if *clear {
            u.bits = 9;
            *maxc = max_code(9, max_bits);
            *clear = false;
            renew = true;
        }
        u.code(renew)
    };
    let Some(first) = next(&mut u, free, &mut maxc, &mut clear) else {
        return Ok(out);
    };
    if first > 255 {
        return Err(Error::Corrupt);
    }
    let mut old = first;
    let mut fin = first as u8;
    out.push(fin);
    let mut stack = Vec::new();
    while let Some(mut code) = next(&mut u, free, &mut maxc, &mut clear) {
        if code == CLEAR && block {
            free = FIRST - 1;
            clear = true;
            match next(&mut u, free, &mut maxc, &mut clear) {
                Some(c) => code = c,
                None => break,
            }
        }
        let incode = code;
        stack.clear();
        let mut c = code;
        if c >= free {
            if c > free {
                return Err(Error::Corrupt);
            }
            stack.push(fin);
            c = old;
        }
        while c >= 256 {
            stack.push(suffix[c as usize]);
            c = prefix[c as usize];
        }
        fin = c as u8;
        stack.push(fin);
        out.extend(stack.iter().rev());
        if free < limit {
            prefix[free as usize] = old;
            suffix[free as usize] = fin;
            free += 1;
        }
        old = incode;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips() {
        let text = b"TOBEORNOTTOBEORTOBEORNOT#".repeat(50);
        for bits in [9, 12, 16] {
            let z = compress(&text, bits);
            assert!(z.len() < text.len());
            assert_eq!(decompress(&z).unwrap(), text);
        }
        assert_eq!(decompress(&compress(b"", 16)).unwrap(), b"");
        assert_eq!(decompress(&compress(b"a", 16)).unwrap(), b"a");
        assert_eq!(decompress(b"xx"), Err(Error::NotCompressed));
    }

    #[test]
    fn known_bytes() {
        // "aaa": codes 97, 257 at 9 bits.
        assert_eq!(compress(b"aaa", 16), vec![0x1f, 0x9d, 0x90, 0x61, 0x02, 0x02]);
    }
}
