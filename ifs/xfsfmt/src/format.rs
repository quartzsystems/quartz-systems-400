// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Writing a new filesystem onto a device.

use std::io::{Seek, SeekFrom, Write};

use uuid::Uuid;

use crate::geom::{BLOCK_SIZE, CHUNK_INODES, Geometry, INODE_SIZE};
use crate::layout::ag::{self, Inodes};
use crate::layout::btree::{self, InoRec};
use crate::layout::inode::{self, Kind};
use crate::layout::{log, sb};
use crate::{Error, LABEL};

/// Inodes the first chunk starts with in use: root, rt bitmap, rt summary.
const USED: u32 = 3;

/// A point in time for the inodes mkfs creates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Timestamp {
    /// Seconds since the Unix epoch.
    pub secs: i64,
    /// Nanoseconds.
    pub nsecs: u32,
}

/// What varies between two filesystems of the same size.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Options {
    /// Filesystem UUID: fixed for built images, random at install.
    pub uuid: Uuid,
    /// Volume label, at most 12 bytes.
    pub label: String,
    /// Sector size, 512 or 4096; at least the device's logical block size.
    pub sector_size: u32,
    /// Creation time of the root directory.
    pub time: Timestamp,
    /// The device already reads as zeros (a new image file), so the log
    /// need not be cleared.
    pub zeroed: bool,
}

impl Options {
    /// Label [`LABEL`], 512-byte sectors, a device that is not known zero.
    #[must_use]
    pub fn new(uuid: Uuid, time: Timestamp) -> Self {
        Self {
            uuid,
            label: LABEL.to_string(),
            sector_size: 512,
            time,
            zeroed: false,
        }
    }
}

/// Write an empty filesystem onto the first `bytes` of `dev`. The
/// primary superblock goes last, so an interrupted format never mounts.
pub fn format<D: Write + Seek>(dev: &mut D, bytes: u64, opts: &Options) -> Result<Geometry, Error> {
    let g = Geometry::for_bytes(bytes, opts.sector_size)?;
    if opts.label.len() > 12 {
        return Err(Error::Label(opts.label.len()));
    }
    let uuid = *opts.uuid.as_bytes();
    let sect = g.sector_size as usize;

    // Kill any earlier primary superblock first.
    put(dev, 0, &vec![0u8; sect])?;

    // The log.
    let log_at = g.offset(g.log_ag, g.log_agbno);
    if !opts.zeroed {
        let zeros = vec![0u8; 1024 * 1024];
        let len = u64::from(g.log_blocks) * u64::from(BLOCK_SIZE);
        let mut done = 0;
        while done < len {
            let n = (len - done).min(zeros.len() as u64);
            put(
                dev,
                log_at + done,
                &zeros[..usize::try_from(n).expect("1 MiB")],
            )?;
            done += n;
        }
    }
    put(dev, log_at, &log::clean_record(g.sector_size, &uuid))?;

    let primary = sb::build(
        &g,
        &uuid,
        opts.label.as_bytes(),
        &sb::Counts {
            icount: u64::from(CHUNK_INODES),
            ifree: u64::from(CHUNK_INODES - USED),
        },
    );
    let root = g.root_ino();
    let chunk = InoRec {
        start: u32::try_from(root).expect("AG 0"),
        free_count: CHUNK_INODES - USED,
        free: !0u64 << USED,
    };

    for agno in 0..g.ag_count {
        write_ag(dev, &g, agno, &uuid, &primary, chunk)?;
    }

    // The first inode chunk.
    let mut inodes = Vec::with_capacity((CHUNK_INODES * INODE_SIZE) as usize);
    for i in 0..u64::from(CHUNK_INODES) {
        let kind = match i {
            0 => Kind::Root,
            1 => Kind::RtBitmap,
            2 => Kind::RtSummary,
            _ => Kind::Free,
        };
        inodes.extend(inode::build(kind, root + i, root, opts.time, &uuid));
    }
    put(dev, g.offset(0, g.chunk_agbno), &inodes)?;

    dev.flush().map_err(Error::io("flush"))?;
    put(dev, 0, &primary)?;
    dev.flush().map_err(Error::io("flush"))?;
    Ok(g)
}

/// One AG's headers and btree roots. AG 0's superblock sector is left
/// zero here; `primary` is copied into every other AG's.
fn write_ag<D: Write + Seek>(
    dev: &mut D,
    g: &Geometry,
    agno: u32,
    uuid: &[u8; 16],
    primary: &[u8],
    chunk: InoRec,
) -> Result<(), Error> {
    // Headers: superblock, AGF, AGI, AGFL in sectors 0 to 3.
    let sect = g.sector_size as usize;
    let mut head = vec![0u8; (g.agfl_block() as usize + 1) * BLOCK_SIZE as usize];
    if agno != 0 {
        head[..sect].copy_from_slice(primary);
    }
    let inodes = if agno == 0 {
        Inodes {
            count: CHUNK_INODES,
            free: CHUNK_INODES - USED,
            newino: Some(chunk.start),
        }
    } else {
        Inodes {
            count: 0,
            free: 0,
            newino: None,
        }
    };
    head[sect..2 * sect].copy_from_slice(&ag::build_agf(g, agno, uuid));
    head[2 * sect..3 * sect].copy_from_slice(&ag::build_agi(g, agno, uuid, &inodes));
    head[3 * sect..4 * sect].copy_from_slice(&ag::build_agfl(g, agno, uuid));
    put(dev, g.offset(agno, 0), &head)?;

    // Btree roots, one leaf each.
    let [bno, cnt, ino, fino] = g.roots();
    let by_bno = g.free_extents(agno);
    let mut by_cnt = by_bno.clone();
    by_cnt.sort_by_key(|&(start, len)| (len, start));
    let recs: &[InoRec] = if agno == 0 { &[chunk] } else { &[] };
    let leaf = |agbno| (g.daddr(agno, agbno), uuid, agno);
    let blocks = [
        (bno, btree::alloc_leaf(btree::BNO_MAGIC, &by_bno, leaf(bno))),
        (cnt, btree::alloc_leaf(btree::CNT_MAGIC, &by_cnt, leaf(cnt))),
        (ino, btree::ino_leaf(btree::INO_MAGIC, recs, leaf(ino))),
        (fino, btree::ino_leaf(btree::FINO_MAGIC, recs, leaf(fino))),
    ];
    for (agbno, block) in blocks {
        put(dev, g.offset(agno, agbno), &block)?;
    }
    Ok(())
}

fn put<D: Write + Seek>(dev: &mut D, at: u64, data: &[u8]) -> Result<(), Error> {
    dev.seek(SeekFrom::Start(at)).map_err(Error::io("seek"))?;
    dev.write_all(data).map_err(Error::io("write"))
}
