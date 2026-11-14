// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The write-intent map in memory, and the versioned bytes a caller
//! may keep it in.
//!
//! The bytes are a 64-byte header and one bit per region, region 0 the
//! low bit of the first byte. A bit is changed in place: setting or
//! clearing one rewrites the byte that holds it and nothing else.

use crate::bytes::{get_u32, get_u64, put_u32, put_u64};
use crate::spec::{Layout, field};

/// Bytes of the header; the bits follow.
pub const HEADER_LEN: usize = 64;
/// The serialization's version.
pub const VERSION: u32 = 1;
/// The default region: 16 MiB.
pub const DEFAULT_REGION_BYTES: u64 = 16 << 20;

const OFF_VERSION: usize = 8;
const OFF_KIND: usize = 12;
const OFF_REGION: usize = 16;
const OFF_REGIONS: usize = 24;
const OFF_SET: usize = 32;
const OFF_MEMBER: usize = 48;

/// The header's layout.
pub static HEADER_LAYOUT: Layout = Layout {
    name: "IntentHeader",
    size: Some(HEADER_LEN),
    fields: &[
        field("magic", 0, 8),
        field("version", OFF_VERSION, 4),
        field("kind", OFF_KIND, 4),
        field("region_blocks", OFF_REGION, 8),
        field("regions", OFF_REGIONS, 8),
        field("set_id", OFF_SET, 16),
        field("member", OFF_MEMBER, 16),
    ],
};

/// What a map records.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MapKind {
    /// Nothing: a map given up.
    Off,
    /// The set's own map: regions that may differ between its members
    /// after an unclean stop.
    Set,
    /// Kept for a member out of service that was in step when it left:
    /// every region written since. Its own guide back.
    Away,
    /// Kept for a member that left while it was being resumed: the
    /// regions written since, which the resume's own map must copy
    /// again. No guide without that map.
    AwayResuming,
}

impl MapKind {
    fn as_u32(self) -> u32 {
        match self {
            MapKind::Off => 0,
            MapKind::Set => 1,
            MapKind::Away => 2,
            MapKind::AwayResuming => 3,
        }
    }

    fn from_u32(v: u32) -> Option<Self> {
        Some(match v {
            0 => MapKind::Off,
            1 => MapKind::Set,
            2 => MapKind::Away,
            3 => MapKind::AwayResuming,
            _ => return None,
        })
    }
}

/// A write-intent map: one bit per region of a set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IntentMap {
    /// What it records.
    pub kind: MapKind,
    /// The set.
    pub set_id: [u8; 16],
    /// The member it is kept for; zeros for the set's own map.
    pub member: [u8; 16],
    /// Blocks per region.
    pub region_blocks: u64,
    /// Regions in the set.
    pub regions: u64,
    bits: Vec<u8>,
}

impl IntentMap {
    /// Regions of a set of `blocks` blocks.
    #[must_use]
    pub fn regions_for(blocks: u64, region_blocks: u64) -> u64 {
        blocks.div_ceil(region_blocks)
    }

    /// A map with no region set.
    #[must_use]
    pub fn new(
        kind: MapKind,
        set_id: [u8; 16],
        member: [u8; 16],
        blocks: u64,
        region_blocks: u64,
    ) -> Self {
        let regions = Self::regions_for(blocks, region_blocks);
        Self {
            kind,
            set_id,
            member,
            region_blocks,
            regions,
            bits: vec![0; usize::try_from(regions.div_ceil(8)).expect("fits")],
        }
    }

    /// Whether this map was made for the set and member given.
    #[must_use]
    pub fn fits(
        &self,
        set_id: [u8; 16],
        member: [u8; 16],
        blocks: u64,
        region_blocks: u64,
    ) -> bool {
        self.set_id == set_id
            && self.member == member
            && self.region_blocks == region_blocks
            && self.regions == Self::regions_for(blocks, region_blocks)
    }

    fn at(r: u64) -> (usize, u8) {
        (usize::try_from(r / 8).expect("fits"), 1 << (r % 8))
    }

    /// Whether region `r` is set.
    #[must_use]
    pub fn is_set(&self, r: u64) -> bool {
        let (i, b) = Self::at(r);
        self.bits[i] & b != 0
    }

    /// Set region `r`; whether it changed.
    pub fn set(&mut self, r: u64) -> bool {
        let (i, b) = Self::at(r);
        let was = self.bits[i] & b != 0;
        self.bits[i] |= b;
        !was
    }

    /// Clear region `r`; whether it changed.
    pub fn clear(&mut self, r: u64) -> bool {
        let (i, b) = Self::at(r);
        let was = self.bits[i] & b != 0;
        self.bits[i] &= !b;
        was
    }

    /// Set every region.
    pub fn set_all(&mut self) {
        for r in 0..self.regions {
            self.set(r);
        }
    }

    /// Take the regions set in `other`, a map of the same set.
    pub fn merge(&mut self, other: &IntentMap) {
        for (a, b) in self.bits.iter_mut().zip(&other.bits) {
            *a |= b;
        }
    }

    /// The regions set, in order. Bytes with nothing set are passed
    /// over whole.
    pub fn dirty(&self) -> impl Iterator<Item = u64> + '_ {
        self.bits
            .iter()
            .enumerate()
            .filter(|(_, b)| **b != 0)
            .flat_map(|(i, b)| {
                (0..8u64)
                    .filter(move |k| b >> k & 1 == 1)
                    .map(move |k| i as u64 * 8 + k)
            })
    }

    /// Regions set.
    #[must_use]
    pub fn count(&self) -> u64 {
        self.bits.iter().map(|b| u64::from(b.count_ones())).sum()
    }

    /// The offset in the kept bytes of the byte holding region `r`.
    #[must_use]
    pub fn offset_of(r: u64) -> u64 {
        HEADER_LEN as u64 + r / 8
    }

    /// The kept bytes holding regions `lo..=hi`: their offset and the
    /// bytes as they are now.
    #[must_use]
    pub fn span(&self, lo: u64, hi: u64) -> (u64, &[u8]) {
        let (a, _) = Self::at(lo);
        let (b, _) = Self::at(hi);
        (Self::offset_of(lo), &self.bits[a..=b])
    }

    /// The header under the caller's magic.
    #[must_use]
    pub fn header(&self, magic: &[u8; 8]) -> [u8; HEADER_LEN] {
        let mut h = [0u8; HEADER_LEN];
        h[..8].copy_from_slice(magic);
        put_u32(&mut h, OFF_VERSION, VERSION);
        put_u32(&mut h, OFF_KIND, self.kind.as_u32());
        put_u64(&mut h, OFF_REGION, self.region_blocks);
        put_u64(&mut h, OFF_REGIONS, self.regions);
        h[OFF_SET..OFF_SET + 16].copy_from_slice(&self.set_id);
        h[OFF_MEMBER..OFF_MEMBER + 16].copy_from_slice(&self.member);
        h
    }

    /// The whole map as kept: header, then bits.
    #[must_use]
    pub fn encode(&self, magic: &[u8; 8]) -> Vec<u8> {
        let mut out = Vec::with_capacity(HEADER_LEN + self.bits.len());
        out.extend_from_slice(&self.header(magic));
        out.extend_from_slice(&self.bits);
        out
    }

    /// A map from kept bytes under the caller's magic; `None` when they
    /// are not one (another magic or version, cut short).
    #[must_use]
    pub fn decode(magic: &[u8; 8], bytes: &[u8]) -> Option<Self> {
        if bytes.len() < HEADER_LEN
            || &bytes[..8] != magic
            || get_u32(bytes, OFF_VERSION) != VERSION
        {
            return None;
        }
        let kind = MapKind::from_u32(get_u32(bytes, OFF_KIND))?;
        let region_blocks = get_u64(bytes, OFF_REGION);
        let regions = get_u64(bytes, OFF_REGIONS);
        let need = usize::try_from(regions.div_ceil(8)).ok()?;
        if region_blocks == 0 || bytes.len() < HEADER_LEN.checked_add(need)? {
            return None;
        }
        let mut bits = bytes[HEADER_LEN..HEADER_LEN + need].to_vec();
        if !regions.is_multiple_of(8)
            && let Some(last) = bits.last_mut()
        {
            *last &= (1u8 << (regions % 8)) - 1;
        }
        Some(Self {
            kind,
            set_id: bytes[OFF_SET..OFF_SET + 16].try_into().ok()?,
            member: bytes[OFF_MEMBER..OFF_MEMBER + 16].try_into().ok()?,
            region_blocks,
            regions,
            bits,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MAGIC: &[u8; 8] = b"TESTWIBM";

    #[test]
    fn a_map_round_trips_and_names_its_bytes() {
        let mut m = IntentMap::new(MapKind::Set, [1; 16], [0; 16], 100_000, 4096);
        assert_eq!(m.regions, 25);
        assert!(m.set(3) && !m.set(3));
        m.set(17);
        m.set(24);
        assert_eq!(m.dirty().collect::<Vec<_>>(), [3, 17, 24]);
        assert_eq!(m.count(), 3);
        let bytes = m.encode(MAGIC);
        assert_eq!(bytes.len(), HEADER_LEN + 4);
        assert_eq!(IntentMap::decode(MAGIC, &bytes).unwrap(), m);
        assert!(IntentMap::decode(b"OTHERMAP", &bytes).is_none());
        assert!(IntentMap::decode(MAGIC, &bytes[..HEADER_LEN + 3]).is_none());
        assert!(m.fits([1; 16], [0; 16], 100_000, 4096));
        assert!(!m.fits([1; 16], [0; 16], 200_000, 4096));

        // A bit's byte, where the kept bytes have it.
        let (off, b) = m.span(17, 17);
        assert_eq!((off, b), (HEADER_LEN as u64 + 2, &[0b10][..]));
        assert_eq!(bytes[usize::try_from(off).unwrap()], 0b10);
        assert!(m.clear(17) && !m.clear(17));
        assert_eq!(m.span(0, 24).1, &[0b1000, 0, 0, 1]);

        // Bits past the last region are not regions.
        let mut tail = m.encode(MAGIC);
        *tail.last_mut().unwrap() = 0xFF;
        assert_eq!(IntentMap::decode(MAGIC, &tail).unwrap().count(), 2);

        let mut away = IntentMap::new(MapKind::Away, [1; 16], [7; 16], 100_000, 4096);
        away.merge(&m);
        assert_eq!(away.dirty().collect::<Vec<_>>(), [3, 24]);
        let back = IntentMap::decode(MAGIC, &away.encode(MAGIC)).unwrap();
        assert_eq!((back.kind, back.member), (MapKind::Away, [7; 16]));
    }

    #[test]
    fn the_header_is_as_the_table_says() {
        let mut end = 0;
        for f in HEADER_LAYOUT.fields {
            assert_eq!(f.offset, Some(end), "{}", f.name);
            end += f.size.unwrap();
        }
        assert_eq!(end, HEADER_LEN);
    }
}
