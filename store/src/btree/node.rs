// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Node page encoding. See `docs/STORE_FORMAT.md` §3. A node is decoded
//! into vectors, changed there, and encoded into a fresh page: with
//! copy-on-write every modified node is rewritten anyway, so there is no
//! in-place slot management.

use crate::bytes::{get_u16, get_u64, put_u16, put_u64, u16_of};
use crate::page::BODY_SIZE;
use crate::page::pager::PageNo;
use crate::spec::{Field, Layout, field};
use crate::{Result, StoreError};

use super::codec::FixedKey;

/// Bytes of node header.
pub const HEADER_LEN: usize = 16;
/// Longest key.
pub const MAX_KEY_LEN: usize = 256;
/// Longest value.
pub const MAX_VALUE_LEN: usize = 1024;
/// Bytes per leaf slot beyond the key.
pub const SLOT_EXTRA: usize = 4;
/// Bytes per internal entry beyond the key.
pub const ENTRY_EXTRA: usize = 8;
/// A node using less than this many bytes is underfull.
pub const MIN_USED: usize = BODY_SIZE / 4;
/// Bulk load fills nodes to this many bytes.
pub const BULK_FILL: usize = BODY_SIZE * 9 / 10;

const OFF_LEVEL: usize = 0;
const OFF_FLAGS: usize = 1;
const OFF_COUNT: usize = 2;
const OFF_KEY_LEN: usize = 4;
const OFF_FIRST_CHILD: usize = HEADER_LEN;

/// Header layout for the spec test.
pub static HEADER_LAYOUT: Layout = Layout {
    name: "NodeHeader",
    size: Some(HEADER_LEN),
    fields: &[
        field("level", OFF_LEVEL, 1),
        field("flags", OFF_FLAGS, 1),
        field("count", OFF_COUNT, 2),
        field("key_len", OFF_KEY_LEN, 2),
        field("reserved", 6, 2),
        field("reserved2", 8, 8),
    ],
};

/// Internal entry layout for the spec test.
pub static INTERNAL_ENTRY_LAYOUT: Layout = Layout {
    name: "InternalEntry",
    size: None,
    fields: &[
        Field {
            name: "key",
            offset: Some(0),
            size: None,
        },
        Field {
            name: "child",
            offset: None,
            size: Some(8),
        },
    ],
};

/// Leaf slot layout for the spec test.
pub static LEAF_SLOT_LAYOUT: Layout = Layout {
    name: "LeafSlot",
    size: None,
    fields: &[
        Field {
            name: "key",
            offset: Some(0),
            size: None,
        },
        Field {
            name: "value_off",
            offset: None,
            size: Some(2),
        },
        Field {
            name: "value_len",
            offset: None,
            size: Some(2),
        },
    ],
};

/// A decoded node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Node<K> {
    /// Keys with their values, ascending.
    Leaf(Vec<(K, Vec<u8>)>),
    /// `first` holds keys below `entries[0].0`; `entries[i].1` holds keys
    /// from `entries[i].0` up to the next separator.
    Internal {
        /// Leftmost child.
        first: PageNo,
        /// Separator keys and the child to their right, ascending.
        entries: Vec<(K, PageNo)>,
    },
}

fn corrupt(page: PageNo, what: impl Into<String>) -> StoreError {
    StoreError::Corrupt {
        page,
        what: what.into(),
    }
}

impl<K: FixedKey> Node<K> {
    /// Bytes this node occupies when encoded.
    pub fn used(&self) -> usize {
        match self {
            Self::Leaf(e) => {
                HEADER_LEN
                    + e.len() * (K::LEN + SLOT_EXTRA)
                    + e.iter().map(|(_, v)| v.len()).sum::<usize>()
            }
            Self::Internal { entries, .. } => {
                HEADER_LEN + 8 + entries.len() * (K::LEN + ENTRY_EXTRA)
            }
        }
    }

    /// Whether the node fits in a page.
    pub fn fits(&self) -> bool {
        self.used() <= BODY_SIZE
    }

    /// Whether the node is small enough to merge.
    pub fn underfull(&self) -> bool {
        self.used() < MIN_USED
    }

    /// Number of keys.
    pub fn count(&self) -> usize {
        match self {
            Self::Leaf(e) => e.len(),
            Self::Internal { entries, .. } => entries.len(),
        }
    }

    /// Lowest key, if any.
    pub fn first_key(&self) -> Option<&K> {
        match self {
            Self::Leaf(e) => e.first().map(|(k, _)| k),
            Self::Internal { entries, .. } => entries.first().map(|(k, _)| k),
        }
    }

    /// Encode into a page body. The node must fit.
    pub fn encode(&self, body: &mut [u8]) {
        debug_assert!(self.fits(), "node does not fit");
        body.fill(0);
        put_u16(body, OFF_KEY_LEN, u16_of(K::LEN));
        match self {
            Self::Leaf(entries) => {
                body[OFF_LEVEL] = 0;
                put_u16(body, OFF_COUNT, u16_of(entries.len()));
                let mut slot = HEADER_LEN;
                let mut end = BODY_SIZE;
                for (k, v) in entries {
                    end -= v.len();
                    body[end..end + v.len()].copy_from_slice(v);
                    k.write(&mut body[slot..slot + K::LEN]);
                    put_u16(body, slot + K::LEN, u16_of(end));
                    put_u16(body, slot + K::LEN + 2, u16_of(v.len()));
                    slot += K::LEN + SLOT_EXTRA;
                }
            }
            Self::Internal { first, entries } => {
                body[OFF_LEVEL] = 1;
                put_u16(body, OFF_COUNT, u16_of(entries.len()));
                put_u64(body, OFF_FIRST_CHILD, *first);
                let mut at = OFF_FIRST_CHILD + 8;
                for (k, child) in entries {
                    k.write(&mut body[at..at + K::LEN]);
                    put_u64(body, at + K::LEN, *child);
                    at += K::LEN + ENTRY_EXTRA;
                }
            }
        }
    }

    /// Decode a page body. `page` is for error messages only.
    pub fn decode(body: &[u8], page: PageNo) -> Result<Self> {
        let key_len = usize::from(get_u16(body, OFF_KEY_LEN));
        if key_len != K::LEN {
            return Err(corrupt(
                page,
                format!("key length {key_len}, tree uses {}", K::LEN),
            ));
        }
        let count = usize::from(get_u16(body, OFF_COUNT));
        match body[OFF_LEVEL] {
            0 => {
                if HEADER_LEN + count * (K::LEN + SLOT_EXTRA) > BODY_SIZE {
                    return Err(corrupt(page, format!("{count} slots do not fit")));
                }
                let mut entries = Vec::with_capacity(count);
                let mut slot = HEADER_LEN;
                let mut prev: Option<K> = None;
                for _ in 0..count {
                    let k = K::read(&body[slot..slot + K::LEN]);
                    let off = usize::from(get_u16(body, slot + K::LEN));
                    let len = usize::from(get_u16(body, slot + K::LEN + 2));
                    if len > MAX_VALUE_LEN
                        || off + len > BODY_SIZE
                        || off < HEADER_LEN + count * (K::LEN + SLOT_EXTRA)
                    {
                        return Err(corrupt(page, format!("value at {off}+{len} out of bounds")));
                    }
                    if prev.as_ref().is_some_and(|p| p >= &k) {
                        return Err(corrupt(page, "leaf keys not ascending"));
                    }
                    entries.push((k.clone(), body[off..off + len].to_vec()));
                    prev = Some(k);
                    slot += K::LEN + SLOT_EXTRA;
                }
                Ok(Self::Leaf(entries))
            }
            1 => {
                if HEADER_LEN + 8 + count * (K::LEN + ENTRY_EXTRA) > BODY_SIZE {
                    return Err(corrupt(page, format!("{count} entries do not fit")));
                }
                let first = get_u64(body, OFF_FIRST_CHILD);
                let mut entries = Vec::with_capacity(count);
                let mut at = OFF_FIRST_CHILD + 8;
                let mut prev: Option<K> = None;
                for _ in 0..count {
                    let k = K::read(&body[at..at + K::LEN]);
                    if prev.as_ref().is_some_and(|p| p >= &k) {
                        return Err(corrupt(page, "internal keys not ascending"));
                    }
                    entries.push((k.clone(), get_u64(body, at + K::LEN)));
                    prev = Some(k);
                    at += K::LEN + ENTRY_EXTRA;
                }
                Ok(Self::Internal { first, entries })
            }
            l => Err(corrupt(page, format!("node level {l}"))),
        }
    }

    /// Split a node that no longer fits into two of roughly equal size.
    /// Returns the right half; `self` keeps the left. For an internal
    /// node the middle key is returned separately: it moves up.
    pub fn split(&mut self) -> (Option<K>, Self) {
        match self {
            Self::Leaf(entries) => {
                let total: usize = entries
                    .iter()
                    .map(|(_, v)| K::LEN + SLOT_EXTRA + v.len())
                    .sum();
                let mut acc = 0;
                let mut at = 0;
                for (i, (_, v)) in entries.iter().enumerate() {
                    acc += K::LEN + SLOT_EXTRA + v.len();
                    if acc * 2 >= total {
                        at = i + 1;
                        break;
                    }
                }
                let at = at.clamp(1, entries.len() - 1);
                let right = entries.split_off(at);
                (None, Self::Leaf(right))
            }
            Self::Internal { entries, .. } => {
                let m = entries.len() / 2;
                let mut right = entries.split_off(m);
                let (up, right_first) = right.remove(0);
                (
                    Some(up),
                    Self::Internal {
                        first: right_first,
                        entries: right,
                    },
                )
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn leaf_round_trip_and_bounds() {
        let n: Node<u64> = Node::Leaf(vec![(1, vec![9; 3]), (2, vec![]), (7, vec![1, 2])]);
        let mut body = vec![0u8; BODY_SIZE];
        n.encode(&mut body);
        assert_eq!(Node::<u64>::decode(&body, 3).unwrap(), n);
        assert_eq!(n.used(), HEADER_LEN + 3 * 12 + 5);
        // A tampered slot offset is caught.
        put_u16(&mut body, HEADER_LEN + 8, 5000);
        assert!(matches!(
            Node::<u64>::decode(&body, 3),
            Err(StoreError::Corrupt { page: 3, .. })
        ));
        // Wrong key size for this tree.
        n.encode(&mut body);
        assert!(matches!(
            Node::<ObjectIdKey>::decode(&body, 4),
            Err(StoreError::Corrupt { .. })
        ));
    }

    type ObjectIdKey = crate::obj::ObjectId;

    #[test]
    fn internal_round_trip_and_split() {
        let entries: Vec<(u64, PageNo)> = (1..=9).map(|i| (i * 10, 100 + i)).collect();
        let n = Node::Internal { first: 99, entries };
        let mut body = vec![0u8; BODY_SIZE];
        n.encode(&mut body);
        assert_eq!(Node::<u64>::decode(&body, 1).unwrap(), n);
        let mut left = n;
        let (up, right) = left.split();
        assert_eq!(up, Some(50));
        assert_eq!(
            left,
            Node::Internal {
                first: 99,
                entries: (1..=4).map(|i| (i * 10, 100 + i)).collect()
            }
        );
        assert_eq!(
            right,
            Node::Internal {
                first: 105,
                entries: (6..=9).map(|i| (i * 10, 100 + i)).collect()
            }
        );
    }

    #[test]
    fn leaf_split_balances_bytes() {
        let mut big: Node<u64> = Node::Leaf(vec![
            (1, vec![0; 1000]),
            (2, vec![0; 10]),
            (3, vec![0; 10]),
            (4, vec![0; 10]),
        ]);
        let (up, right) = big.split();
        assert!(up.is_none());
        assert_eq!(big.count(), 1);
        assert_eq!(right.count(), 3);
        assert_eq!(right.first_key(), Some(&2));
    }
}
