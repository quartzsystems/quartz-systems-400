// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Double-byte tables: a set is layers searched in order (a mixed
//! CCSID's differences over code page 300), with the reverse map built
//! once per set.

use std::collections::HashMap;
use std::fmt;
use std::sync::OnceLock;

use crate::tables::{DB0300, DB0930, DB0939};

/// One generated double-byte table.
#[derive(Debug)]
pub struct Layer {
    /// (bytes, code point), sorted by bytes.
    pub pairs: &'static [(u16, u16)],
    /// (code point, bytes) for code points that only encode.
    pub fallbacks: &'static [(u16, u16)],
}

/// A double-byte character set.
pub struct DbcsTable {
    /// The CCSID it was generated for; equal sets are the same table.
    pub set: u16,
    layers: &'static [&'static Layer],
    rev: OnceLock<HashMap<u32, u16>>,
}

/// Code page 300.
pub static JA300: DbcsTable = DbcsTable {
    set: 300,
    layers: &[&DB0300],
    rev: OnceLock::new(),
};
/// The double-byte half of 930 and 5026.
pub static JA930: DbcsTable = DbcsTable {
    set: 930,
    layers: &[&DB0930, &DB0300],
    rev: OnceLock::new(),
};
/// The double-byte half of 939 and 5035.
pub static JA939: DbcsTable = DbcsTable {
    set: 939,
    layers: &[&DB0939, &DB0300],
    rev: OnceLock::new(),
};

impl DbcsTable {
    /// The character of a pair, if mapped.
    #[must_use]
    pub fn decode(&self, pair: u16) -> Option<char> {
        self.layers.iter().find_map(|l| {
            l.pairs
                .binary_search_by_key(&pair, |p| p.0)
                .ok()
                .and_then(|i| char::from_u32(u32::from(l.pairs[i].1)))
        })
    }

    /// The pair of a character, if it has one.
    #[must_use]
    pub fn encode(&self, ch: char) -> Option<u16> {
        self.reverse().get(&u32::from(ch)).copied()
    }

    /// Every pair that decodes, with its character.
    pub fn pairs(&self) -> impl Iterator<Item = (u16, char)> + '_ {
        (0..=u16::MAX).filter_map(|p| self.decode(p).map(|c| (p, c)))
    }

    /// Lower layers first, so an upper layer's pair wins.
    fn reverse(&self) -> &HashMap<u32, u16> {
        self.rev.get_or_init(|| {
            let mut m = HashMap::new();
            for l in self.layers.iter().rev() {
                for &(b, cp) in l.pairs {
                    m.insert(u32::from(cp), b);
                }
                for &(cp, b) in l.fallbacks {
                    m.insert(u32::from(cp), b);
                }
            }
            m
        })
    }
}

impl fmt::Debug for DbcsTable {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "DbcsTable({})", self.set)
    }
}

impl PartialEq for DbcsTable {
    fn eq(&self, other: &Self) -> bool {
        self.set == other.set
    }
}

impl Eq for DbcsTable {}
