// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Partial-parity slots: what an epoch is about to write, so a crash in
//! the middle can be repaired even with a unit missing. See
//! `docs/STORE_FORMAT.md` §11, `PplHeader` and `PplEntry`.

use crc::{CRC_32_ISCSI, Crc, Table};

use crate::bytes::{get_u32, get_u64, put_u32, put_u64};
use crate::asp::parity::geometry::PPL_ENTRIES;
use crate::asp::parity::table::Level;
use crate::page::PAGE_SIZE;
use crate::spec::{Layout, field};

const CRC32C: Crc<u32, Table<16>> = Crc::<u32, Table<16>>::new(&CRC_32_ISCSI);

/// Magic at offset 0.
pub const MAGIC: &[u8; 8] = b"QS400PPL";
/// Header bytes; entries follow.
pub const HEADER_LEN: usize = 64;
/// Bytes per entry.
pub const ENTRY_LEN: usize = 16;

const OFF_SEQ: usize = 8;
const OFF_ASP: usize = 16;
const OFF_SET: usize = 32;
const OFF_LEVEL: usize = 33;
const OFF_MEMBERS: usize = 34;
const OFF_MEMBER: usize = 35;
const OFF_COUNT: usize = 36;
const OFF_BLOCKS: usize = 40;
const OFF_CRC: usize = 48;

/// The spec layout of the header.
pub static HEADER_LAYOUT: Layout = Layout {
    name: "PplHeader",
    size: Some(HEADER_LEN),
    fields: &[
        field("magic", 0, 8),
        field("seq", OFF_SEQ, 8),
        field("asp_uuid", OFF_ASP, 16),
        field("set", OFF_SET, 1),
        field("level", OFF_LEVEL, 1),
        field("members", OFF_MEMBERS, 1),
        field("member", OFF_MEMBER, 1),
        field("count", OFF_COUNT, 4),
        field("blocks", OFF_BLOCKS, 8),
        field("crc", OFF_CRC, 4),
        field("reserved", 52, 12),
    ],
};

/// The spec layout of an entry.
pub static ENTRY_LAYOUT: Layout = Layout {
    name: "PplEntry",
    size: Some(ENTRY_LEN),
    fields: &[
        field("stripe", 0, 8),
        field("written", 8, 4),
        field("reserved", 12, 4),
    ],
};

/// Which set and member a slot belongs to; a slot that names another is
/// ignored.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ident {
    /// The ASP.
    pub asp_uuid: [u8; 16],
    /// Set number.
    pub set: u8,
    /// Level.
    pub level: Level,
    /// Members in the set.
    pub members: u8,
    /// This member.
    pub member: u8,
    /// Blocks in the geometry.
    pub blocks: u64,
}

/// One stripe of an epoch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// The stripe.
    pub stripe: u64,
    /// Bit `m`: member `m`'s data page is written.
    pub written: u32,
    /// P of the pages not written.
    pub p: Vec<u8>,
    /// Q of the pages not written (RAID 6).
    pub q: Option<Vec<u8>>,
}

/// The pages of a slot holding `count` entries.
#[must_use]
pub fn pages_for(count: usize) -> usize {
    1 + 2 * count
}

/// Encode a slot: the header page and the partial pages.
#[must_use]
pub fn encode(id: &Ident, seq: u64, entries: &[Entry]) -> Vec<u8> {
    assert!(entries.len() <= PPL_ENTRIES, "slot holds {PPL_ENTRIES} entries");
    let mut out = vec![0u8; pages_for(entries.len()) * PAGE_SIZE];
    out[..8].copy_from_slice(MAGIC);
    put_u64(&mut out, OFF_SEQ, seq);
    out[OFF_ASP..OFF_ASP + 16].copy_from_slice(&id.asp_uuid);
    out[OFF_SET] = id.set;
    out[OFF_LEVEL] = id.level.as_u8();
    out[OFF_MEMBERS] = id.members;
    out[OFF_MEMBER] = id.member;
    put_u32(&mut out, OFF_COUNT, u32::try_from(entries.len()).expect("small"));
    put_u64(&mut out, OFF_BLOCKS, id.blocks);
    for (i, e) in entries.iter().enumerate() {
        let at = HEADER_LEN + i * ENTRY_LEN;
        put_u64(&mut out, at, e.stripe);
        put_u32(&mut out, at + 8, e.written);
        let p = (1 + 2 * i) * PAGE_SIZE;
        out[p..p + PAGE_SIZE].copy_from_slice(&e.p);
        if let Some(q) = &e.q {
            out[p + PAGE_SIZE..p + 2 * PAGE_SIZE].copy_from_slice(q);
        }
    }
    let crc = CRC32C.checksum(&out);
    put_u32(&mut out, OFF_CRC, crc);
    out
}

/// The entry count and sequence a header page claims for `id`, if it is
/// one of its slots.
#[must_use]
pub fn header(id: &Ident, page: &[u8]) -> Option<(u64, usize)> {
    if &page[..8] != MAGIC
        || page[OFF_ASP..OFF_ASP + 16] != id.asp_uuid
        || page[OFF_SET] != id.set
        || page[OFF_LEVEL] != id.level.as_u8()
        || page[OFF_MEMBERS] != id.members
        || page[OFF_MEMBER] != id.member
        || get_u64(page, OFF_BLOCKS) != id.blocks
    {
        return None;
    }
    let count = usize::try_from(get_u32(page, OFF_COUNT)).ok()?;
    (count <= PPL_ENTRIES).then_some((get_u64(page, OFF_SEQ), count))
}

/// Decode a whole slot (`pages_for(count)` pages) after [`header`].
#[must_use]
pub fn decode(id: &Ident, bytes: &[u8]) -> Option<(u64, Vec<Entry>)> {
    let (seq, count) = header(id, bytes)?;
    if bytes.len() < pages_for(count) * PAGE_SIZE {
        return None;
    }
    let bytes = &bytes[..pages_for(count) * PAGE_SIZE];
    let stored = get_u32(bytes, OFF_CRC);
    let mut d = CRC32C.digest();
    d.update(&bytes[..OFF_CRC]);
    d.update(&[0; 4]);
    d.update(&bytes[OFF_CRC + 4..]);
    if d.finalize() != stored {
        return None;
    }
    let entries = (0..count)
        .map(|i| {
            let at = HEADER_LEN + i * ENTRY_LEN;
            let p = (1 + 2 * i) * PAGE_SIZE;
            Entry {
                stripe: get_u64(bytes, at),
                written: get_u32(bytes, at + 8),
                p: bytes[p..p + PAGE_SIZE].to_vec(),
                q: (id.level == Level::Raid6)
                    .then(|| bytes[p + PAGE_SIZE..p + 2 * PAGE_SIZE].to_vec()),
            }
        })
        .collect();
    Some((seq, entries))
}

/// Magic of the state page.
pub const STATE_MAGIC: &[u8; 8] = b"QS400PST";
const OFF_FAILED: usize = 36;

/// The spec layout of the state page.
pub static STATE_LAYOUT: Layout = Layout {
    name: "ParityState",
    size: Some(HEADER_LEN),
    fields: &[
        field("magic", 0, 8),
        field("seq", OFF_SEQ, 8),
        field("asp_uuid", OFF_ASP, 16),
        field("set", OFF_SET, 1),
        field("level", OFF_LEVEL, 1),
        field("members", OFF_MEMBERS, 1),
        field("reserved1", 35, 1),
        field("failed", OFF_FAILED, 4),
        field("blocks", OFF_BLOCKS, 8),
        field("crc", OFF_CRC, 4),
        field("reserved", 52, 12),
    ],
};

/// Encode the state page: which members have failed. `id.member` is
/// not recorded: every member carries the same page.
#[must_use]
pub fn encode_state(id: &Ident, seq: u64, failed: u32) -> Vec<u8> {
    let mut out = vec![0u8; PAGE_SIZE];
    out[..8].copy_from_slice(STATE_MAGIC);
    put_u64(&mut out, OFF_SEQ, seq);
    out[OFF_ASP..OFF_ASP + 16].copy_from_slice(&id.asp_uuid);
    out[OFF_SET] = id.set;
    out[OFF_LEVEL] = id.level.as_u8();
    out[OFF_MEMBERS] = id.members;
    put_u32(&mut out, OFF_FAILED, failed);
    put_u64(&mut out, OFF_BLOCKS, id.blocks);
    let crc = CRC32C.checksum(&out[..HEADER_LEN]);
    put_u32(&mut out, OFF_CRC, crc);
    out
}

/// Decode a state page: its sequence and failed members.
#[must_use]
pub fn decode_state(id: &Ident, page: &[u8]) -> Option<(u64, u32)> {
    if &page[..8] != STATE_MAGIC
        || page[OFF_ASP..OFF_ASP + 16] != id.asp_uuid
        || page[OFF_SET] != id.set
        || page[OFF_LEVEL] != id.level.as_u8()
        || page[OFF_MEMBERS] != id.members
        || get_u64(page, OFF_BLOCKS) != id.blocks
    {
        return None;
    }
    let mut head = page[..HEADER_LEN].to_vec();
    put_u32(&mut head, OFF_CRC, 0);
    (CRC32C.checksum(&head) == get_u32(page, OFF_CRC))
        .then(|| (get_u64(page, OFF_SEQ), get_u32(page, OFF_FAILED)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id() -> Ident {
        Ident {
            asp_uuid: [9; 16],
            set: 2,
            level: Level::Raid6,
            members: 5,
            member: 3,
            blocks: 77,
        }
    }

    #[test]
    fn round_trip_and_damage() {
        let entries = vec![
            Entry {
                stripe: 12,
                written: 0b101,
                p: vec![1; PAGE_SIZE],
                q: Some(vec![2; PAGE_SIZE]),
            },
            Entry {
                stripe: 40,
                written: 0b10,
                p: vec![3; PAGE_SIZE],
                q: Some(vec![4; PAGE_SIZE]),
            },
        ];
        let mut bytes = encode(&id(), 9, &entries);
        assert_eq!(bytes.len(), 5 * PAGE_SIZE);
        assert_eq!(decode(&id(), &bytes), Some((9, entries)));
        let other = Ident { member: 1, ..id() };
        assert!(decode(&other, &bytes).is_none());
        bytes[3 * PAGE_SIZE + 7] ^= 1;
        assert!(decode(&id(), &bytes).is_none());
    }

    #[test]
    fn state_round_trip() {
        let mut page = encode_state(&id(), 4, 0b100);
        assert_eq!(decode_state(&id(), &page), Some((4, 0b100)));
        page[OFF_FAILED] ^= 1;
        assert!(decode_state(&id(), &page).is_none());
    }
}
