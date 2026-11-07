// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Input mutations: byte-level edits, interesting values, splices with
//! another corpus entry and tokens from the target's dictionary.

/// A small, fast generator (xorshift64*).
#[derive(Clone)]
pub struct Rng(u64);

impl Rng {
    /// Seeded; zero becomes a fixed constant.
    pub fn new(seed: u64) -> Self {
        Self(if seed == 0 {
            0x9E37_79B9_7F4A_7C15
        } else {
            seed
        })
    }

    /// The next 64 bits.
    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// A number below `n` (`n` > 0).
    pub fn below(&mut self, n: usize) -> usize {
        usize::try_from(self.next_u64() % (n.max(1) as u64)).unwrap_or(0)
    }

    /// True one time in `n`.
    pub fn chance(&mut self, n: usize) -> bool {
        self.below(n) == 0
    }
}

const INTERESTING: &[u64] = &[
    0,
    1,
    0x7F,
    0x80,
    0xFF,
    0x100,
    0x7FFF,
    0x8000,
    0xFFFF,
    0x1_0000,
    0x7FFF_FFFF,
    0x8000_0000,
    0xFFFF_FFFF,
    u64::MAX,
];

/// Mutates `data` in place with one to a few edits; never grows past `max`.
pub fn mutate(rng: &mut Rng, data: &mut Vec<u8>, other: &[u8], dict: &[&[u8]], max: usize) {
    let edits = 1 + rng.below(4);
    for _ in 0..edits {
        if data.is_empty() {
            data.push(u8::try_from(rng.below(256)).unwrap_or(0));
            continue;
        }
        let len = data.len();
        match rng.below(12) {
            // A bit flipped.
            0 => {
                let i = rng.below(len);
                data[i] ^= 1 << rng.below(8);
            }
            // A byte set to anything.
            1 => {
                let i = rng.below(len);
                data[i] = u8::try_from(rng.below(256)).unwrap_or(0);
            }
            // A byte nudged.
            2 => {
                let i = rng.below(len);
                let d = u8::try_from(1 + rng.below(16)).unwrap_or(1);
                data[i] = if rng.chance(2) {
                    data[i].wrapping_add(d)
                } else {
                    data[i].wrapping_sub(d)
                };
            }
            // An interesting value, 1, 2, 4 or 8 bytes, either byte order.
            3 => {
                let w = [1, 2, 4, 8][rng.below(4)].min(len);
                let i = rng.below(len - w + 1);
                let v = INTERESTING[rng.below(INTERESTING.len())];
                let bytes = if rng.chance(2) {
                    v.to_be_bytes()
                } else {
                    v.to_le_bytes()
                };
                let src = if rng.chance(2) {
                    &bytes[8 - w..]
                } else {
                    &bytes[..w]
                };
                data[i..i + w].copy_from_slice(src);
            }
            // A run deleted.
            4 | 5 => {
                let i = rng.below(len);
                let n = 1 + rng.below((len - i).min(32));
                data.drain(i..i + n);
            }
            // Random bytes inserted.
            6 => {
                if len < max {
                    let i = rng.below(len + 1);
                    let n = (1 + rng.below(16)).min(max - len);
                    for k in 0..n {
                        data.insert(i + k, u8::try_from(rng.below(256)).unwrap_or(0));
                    }
                }
            }
            // A run copied elsewhere.
            7 => {
                let i = rng.below(len);
                let n = 1 + rng.below((len - i).min(64));
                let chunk: Vec<u8> = data[i..i + n].to_vec();
                let at = rng.below(len + 1);
                if len + n <= max {
                    data.splice(at..at, chunk);
                } else {
                    let at = rng.below(len - n + 1);
                    data[at..at + n].copy_from_slice(&chunk);
                }
            }
            // A dictionary token inserted or written over.
            8 | 9 => {
                if let Some(tok) = (!dict.is_empty()).then(|| dict[rng.below(dict.len())]) {
                    let at = rng.below(len + 1);
                    if rng.chance(2) && len + tok.len() <= max {
                        data.splice(at..at, tok.iter().copied());
                    } else if tok.len() <= len {
                        let at = rng.below(len - tok.len() + 1);
                        data[at..at + tok.len()].copy_from_slice(tok);
                    }
                }
            }
            // Spliced with another entry.
            10 => {
                if !other.is_empty() {
                    let cut = rng.below(len + 1);
                    let from = rng.below(other.len());
                    data.truncate(cut);
                    data.extend_from_slice(&other[from..]);
                    data.truncate(max);
                }
            }
            // Truncated.
            _ => {
                data.truncate(rng.below(len + 1));
            }
        }
    }
}
