// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Delete disk unit data: make a whole disk blank. The whole disk is
//! discarded (on a drive that supports it the blocks then read as
//! zeros), then the first and last [`EDGE_BYTES`] and the head of every
//! partition either GPT listed are written with zeros, so no partition
//! table, unit header, LIC record, FAT, XFS or other signature is left.
//! A drive without discard keeps the old bytes in between; nothing finds
//! them without a partition table.

use blockdev::{AlignedBuf, BLOCK_SIZE, BlockDev, Result};

/// Bytes zeroed at each end of the disk: both GPTs and more.
pub const EDGE_BYTES: u64 = 8 * 1024 * 1024;
/// Bytes zeroed at the start of every partition a GPT listed.
pub const PART_HEAD_BYTES: u64 = 1024 * 1024;
/// Blocks per zero write.
const WRITE_BLOCKS: u64 = 256;
/// Blocks per discard request (1 GiB), one progress step each.
const DISCARD_BLOCKS: u64 = 256 * 1024;
const SECTOR: u64 = 512;
const BS: u64 = BLOCK_SIZE as u64;

/// What a wipe did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Wiped {
    /// Blocks written with zeros.
    pub zeroed: u64,
    /// Blocks discarded.
    pub discarded: u64,
    /// Partitions the GPTs listed.
    pub partitions: usize,
}

/// Wipe `dev`, reporting percent done to `progress`.
pub fn wipe(dev: &dyn BlockDev, progress: &mut dyn FnMut(u8)) -> Result<Wiped> {
    let blocks = dev.blocks();
    let parts = partitions(dev);
    let ranges = zero_ranges(blocks, &parts);
    let zero_steps: u64 = ranges.iter().map(|r| (r.1 - r.0).div_ceil(WRITE_BLOCKS)).sum();
    let discard_steps = blocks.div_ceil(DISCARD_BLOCKS);
    let total = (zero_steps + discard_steps).max(1);
    let mut done = 0u64;
    let mut report = |done: u64| {
        progress(u8::try_from(done * 100 / total).unwrap_or(100));
    };
    report(0);
    let mut out = Wiped {
        partitions: parts.len(),
        ..Wiped::default()
    };
    // Discard first: the zero writes after it are what is guaranteed.
    let mut lba = 0;
    while lba < blocks {
        let n = DISCARD_BLOCKS.min(blocks - lba);
        // Advisory: a drive that refuses keeps its bytes.
        if dev.discard(lba, n).is_ok() {
            out.discarded += n;
        }
        lba += n;
        done += 1;
        report(done);
    }
    let zeros = AlignedBuf::new(usize::try_from(WRITE_BLOCKS).expect("small"));
    for (start, end) in ranges {
        let mut lba = start;
        while lba < end {
            let n = WRITE_BLOCKS.min(end - lba);
            let len = usize::try_from(n * BS).expect("small");
            dev.write(lba, &zeros[..len])?;
            out.zeroed += n;
            lba += n;
            done += 1;
            report(done);
        }
    }
    dev.flush()?;
    progress(100);
    Ok(out)
}

/// Block ranges `[start, end)` to zero: both edges and the partition
/// heads, sorted and merged.
fn zero_ranges(blocks: u64, parts: &[u64]) -> Vec<(u64, u64)> {
    let edge = (EDGE_BYTES / BS).min(blocks);
    let head = PART_HEAD_BYTES / BS;
    let mut r = vec![(0, edge), (blocks - edge, blocks)];
    for &first in parts {
        let start = first * SECTOR / BS;
        if start < blocks {
            r.push((start, (start + head).min(blocks)));
        }
    }
    r.sort_unstable();
    let mut out: Vec<(u64, u64)> = Vec::new();
    for (s, e) in r {
        match out.last_mut() {
            Some(last) if s <= last.1 => last.1 = last.1.max(e),
            _ => out.push((s, e)),
        }
    }
    out
}

/// First LBAs (512-byte sectors) of the partitions the primary and the
/// backup GPT list; unreadable or absent tables give none.
fn partitions(dev: &dyn BlockDev) -> Vec<u64> {
    let blocks = dev.blocks();
    let mut out = Vec::new();
    let last_sector = blocks * BS / SECTOR - 1;
    for header_lba in [1, last_sector] {
        if let Some(h) = read_sectors(dev, header_lba, 1) {
            out.extend(entries(dev, &h));
        }
    }
    out.sort_unstable();
    out.dedup();
    out
}

/// The partitions' first LBAs from the entries a GPT header points at.
fn entries(dev: &dyn BlockDev, header: &[u8]) -> Vec<u64> {
    if header.len() < 92 || &header[..8] != b"EFI PART" {
        return Vec::new();
    }
    let u32_at = |o: usize| u32::from_le_bytes(header[o..o + 4].try_into().expect("4 bytes"));
    let u64_at = |o: usize| u64::from_le_bytes(header[o..o + 8].try_into().expect("8 bytes"));
    let at = u64_at(72);
    let count = u64::from(u32_at(80));
    let size = u64::from(u32_at(84));
    if count == 0 || count > 1024 || !(128..=1024).contains(&size) {
        return Vec::new();
    }
    let sectors = (count * size).div_ceil(SECTOR);
    let Some(table) = read_sectors(dev, at, sectors) else {
        return Vec::new();
    };
    table
        .chunks(usize::try_from(size).expect("small"))
        .take(usize::try_from(count).expect("small"))
        .filter(|e| e[..16].iter().any(|&b| b != 0))
        .map(|e| u64::from_le_bytes(e[32..40].try_into().expect("8 bytes")))
        .collect()
}

/// `count` 512-byte sectors from `lba`, through whole blocks.
fn read_sectors(dev: &dyn BlockDev, lba: u64, count: u64) -> Option<Vec<u8>> {
    let start = lba * SECTOR;
    let end = (lba + count) * SECTOR;
    if end > dev.blocks() * BS {
        return None;
    }
    let first = start / BS;
    let n = end.div_ceil(BS) - first;
    let mut buf = vec![0u8; usize::try_from(n * BS).ok()?];
    dev.read(first, &mut buf).ok()?;
    let off = usize::try_from(start - first * BS).ok()?;
    let len = usize::try_from(end - start).ok()?;
    Some(buf[off..off + len].to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ranges_merge_and_stay_on_the_disk() {
        let blocks = 10_000;
        let edge = EDGE_BYTES / BS;
        // A partition at sector 8 lies inside the first edge.
        let r = zero_ranges(blocks, &[8, 40_000, 1_000_000]);
        assert_eq!(r, vec![(0, edge), (5000, 5256), (blocks - edge, blocks)]);
        // A disk smaller than both edges is zeroed once.
        assert_eq!(zero_ranges(100, &[]), vec![(0, 100)]);
    }
}
