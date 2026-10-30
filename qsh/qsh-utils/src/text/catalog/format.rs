// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The catalog file, QS/400's own layout (big-endian):
//!
//! ```text
//! magic      8 bytes  "QSHCAT01"
//! sets       u32      number of sets, ascending set numbers follow
//! per set:   u32 set number, u32 message count,
//!            per message (ascending): u32 number, u32 length, text bytes
//! ```
//!
//! The text is stored as built (escapes already resolved), without a
//! terminating null.

use std::collections::BTreeMap;

/// The first bytes of a catalog.
pub const MAGIC: &[u8; 8] = b"QSHCAT01";
/// The highest set number.
pub const SET_MAX: u32 = 255;
/// The highest message number.
pub const MSG_MAX: u32 = 32767;
/// The set used when the source names none.
pub const SET_DEFAULT: u32 = 1;

/// A catalog: sets of numbered messages.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Catalog {
    /// Messages by set, then number.
    pub sets: BTreeMap<u32, BTreeMap<u32, Vec<u8>>>,
}

fn u32_at(b: &[u8], at: &mut usize) -> Option<u32> {
    let s = b.get(*at..*at + 4)?;
    *at += 4;
    Some(u32::from_be_bytes([s[0], s[1], s[2], s[3]]))
}

impl Catalog {
    /// Read a catalog; `None` when the bytes are not one.
    pub fn decode(b: &[u8]) -> Option<Self> {
        if b.get(..8)? != MAGIC {
            return None;
        }
        let mut at = 8;
        let mut cat = Self::default();
        for _ in 0..u32_at(b, &mut at)? {
            let set = u32_at(b, &mut at)?;
            let n = u32_at(b, &mut at)?;
            let msgs = cat.sets.entry(set).or_default();
            for _ in 0..n {
                let num = u32_at(b, &mut at)?;
                let len = u32_at(b, &mut at)? as usize;
                let text = b.get(at..at + len)?.to_vec();
                at += len;
                msgs.insert(num, text);
            }
        }
        (at == b.len()).then_some(cat)
    }

    /// The catalog's bytes.
    pub fn encode(&self) -> Vec<u8> {
        let mut out = MAGIC.to_vec();
        out.extend_from_slice(&(self.sets.len() as u32).to_be_bytes());
        for (set, msgs) in &self.sets {
            out.extend_from_slice(&set.to_be_bytes());
            out.extend_from_slice(&(msgs.len() as u32).to_be_bytes());
            for (n, text) in msgs {
                out.extend_from_slice(&n.to_be_bytes());
                out.extend_from_slice(&(text.len() as u32).to_be_bytes());
                out.extend_from_slice(text);
            }
        }
        out
    }

    /// A message.
    pub fn get(&self, set: u32, num: u32) -> Option<&[u8]> {
        self.sets.get(&set)?.get(&num).map(Vec::as_slice)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let mut c = Catalog::default();
        c.sets.entry(1).or_default().insert(2, b"hello".to_vec());
        c.sets.entry(3).or_default().insert(1, Vec::new());
        let b = c.encode();
        assert_eq!(Catalog::decode(&b), Some(c));
        assert_eq!(Catalog::decode(&b[..b.len() - 1]), None);
        assert_eq!(Catalog::decode(b"nope"), None);
    }
}
