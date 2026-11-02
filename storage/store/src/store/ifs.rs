// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The IFS volume at the store: created and grown with a checkpoint
//! each, and read and written by volume page through the units (the
//! format at IPL; the storage service serves it through its shared I/O
//! handle). See `docs/STORE_FORMAT.md` §12.

use std::collections::BTreeMap;
use std::io::{self, Read, Seek, SeekFrom, Write};

use crate::asp::ifs::{IfsIo, Volume};
use crate::asp::superblock::FLAG_IFS_FORMATTED;
use crate::page::PAGE_SIZE;
use crate::store::Store;
use crate::{Refusal, Result, StoreError};

/// A growth step is this fraction of the usable pages (2%).
pub const STEP_DIVISOR: u64 = 50;
/// A new volume is this fraction of the usable pages (10%).
pub const CREATE_DIVISOR: u64 = 10;
/// The filesystem's allocation groups are this fraction of the usable
/// pages, so a volume grown to the whole ASP has about forty.
pub const AG_DIVISOR: u64 = 40;
/// Smallest allocation group: 16 MiB.
const AG_MIN: u64 = 4096;
/// Largest allocation group: 1 TiB.
const AG_MAX: u64 = 1 << 28;

fn not_in_asp() -> StoreError {
    StoreError::Refused {
        unit: None,
        reason: Refusal::IfsNotInAsp,
    }
}

impl Store {
    /// The IFS volume; empty when the IFS is not in the ASP.
    #[must_use]
    pub fn ifs_volume(&self) -> &Volume {
        self.asp.ifs_volume()
    }

    /// The volume for the thread that serves it: transfers through the
    /// units, never through the store.
    pub fn ifs_io(&mut self) -> IfsIo {
        let volume = self.ifs_volume().clone();
        IfsIo::new(self.asp.io().shared(), volume)
    }

    /// Whether the volume holds a formatted filesystem.
    #[must_use]
    pub fn ifs_formatted(&self) -> bool {
        self.asp.superblock().flag(FLAG_IFS_FORMATTED)
    }

    /// Set the smallest growth and the smallest volume asked for at
    /// creation, in pages (tests use small units).
    pub fn set_ifs_limits(&mut self, min: u64, create_min: u64) {
        self.ifs_limits = (min, create_min);
    }

    /// Pages one growth step adds.
    #[must_use]
    pub fn ifs_step(&self) -> u64 {
        self.asp.usable_pages() / STEP_DIVISOR
    }

    /// Pages the volume may still grow by below `threshold` percent.
    #[must_use]
    pub fn ifs_room(&self, threshold: u8) -> u64 {
        self.asp.ifs_room(threshold)
    }

    /// The allocation group size for the volume's filesystem, pages.
    #[must_use]
    pub fn ifs_ag_pages(&self) -> u64 {
        let ag = (self.asp.usable_pages() / AG_DIVISOR).clamp(AG_MIN, AG_MAX);
        ag.min((self.ifs_volume().pages() / 4).max(AG_MIN))
    }

    /// Create the volume: a tenth of the usable pages, at least 1 GiB,
    /// within `threshold`. Returns its pages.
    pub fn ifs_create(&mut self, threshold: u8) -> Result<u64> {
        self.check()?;
        if !self.ifs_volume().is_empty() {
            return Ok(self.ifs_volume().pages());
        }
        let (min, create_min) = self.ifs_limits;
        let want = (self.asp.usable_pages() / CREATE_DIVISOR).max(create_min);
        let room = self.asp.ifs_room(threshold);
        self.asp.ifs_grow(want, room, min)?;
        self.checkpoint()?;
        Ok(self.ifs_volume().pages())
    }

    /// Grow the volume by about `want` pages within `threshold`.
    /// Returns the pages added.
    pub fn ifs_grow(&mut self, want: u64, threshold: u8) -> Result<u64> {
        self.check()?;
        if self.ifs_volume().is_empty() {
            return Err(not_in_asp());
        }
        let room = self.asp.ifs_room(threshold);
        let added = self.asp.ifs_grow(want, room, self.ifs_limits.0)?;
        self.checkpoint()?;
        Ok(added)
    }

    /// Record that the volume holds a filesystem, its writes durable.
    pub fn ifs_set_formatted(&mut self) -> Result<()> {
        self.check()?;
        self.asp.io().flush()?;
        let sb = self.asp.superblock_mut();
        sb.flags |= FLAG_IFS_FORMATTED;
        self.checkpoint()
    }

    /// Read whole volume pages from `v` into `buf`.
    pub fn ifs_read(&mut self, v: u64, buf: &mut [u8]) -> Result<()> {
        let count = (buf.len() / PAGE_SIZE) as u64;
        let runs = self.ifs_volume().runs(v, count).ok_or_else(not_in_asp)?;
        let mut at = 0;
        for (pno, n) in runs {
            let got = self.asp.io().read_run(pno, n)?;
            let len = usize::try_from(n).expect("small") * PAGE_SIZE;
            buf[at..at + len].copy_from_slice(&got[..len]);
            at += len;
        }
        Ok(())
    }

    /// Write whole volume pages from `v`.
    pub fn ifs_write(&mut self, v: u64, bytes: &[u8]) -> Result<()> {
        let count = (bytes.len() / PAGE_SIZE) as u64;
        let runs = self.ifs_volume().runs(v, count).ok_or_else(not_in_asp)?;
        let mut at = 0;
        for (pno, n) in runs {
            let len = usize::try_from(n).expect("small") * PAGE_SIZE;
            let buf = blockdev::AlignedBuf::from_bytes(&bytes[at..at + len]);
            self.asp.io().write_bytes(pno, &buf)?;
            at += len;
        }
        Ok(())
    }

    /// Make the volume's writes durable.
    pub fn ifs_flush(&mut self) -> Result<()> {
        self.asp.io().flush()
    }

    /// The volume as a file.
    pub fn ifs_file(&mut self) -> VolumeFile<'_> {
        VolumeFile {
            store: self,
            pos: 0,
            pages: BTreeMap::new(),
        }
    }
}

/// The volume as a file, for a formatter: reads and writes at any byte
/// offset, pages gathered in memory and written in runs on
/// [`flush`](std::io::Write::flush) (and when many are held).
pub struct VolumeFile<'a> {
    store: &'a mut Store,
    pos: u64,
    pages: BTreeMap<u64, Vec<u8>>,
}

/// Pages a [`VolumeFile`] holds before it writes them out.
const FILE_HELD: usize = 4096;

fn io_err(e: &StoreError) -> io::Error {
    io::Error::other(e.to_string())
}

impl VolumeFile<'_> {
    fn len(&self) -> u64 {
        self.store.ifs_volume().pages() * PAGE_SIZE as u64
    }

    /// Page `v` as it stands: held, else read.
    fn page(&mut self, v: u64) -> io::Result<&mut Vec<u8>> {
        if !self.pages.contains_key(&v) {
            let mut b = vec![0u8; PAGE_SIZE];
            self.store.ifs_read(v, &mut b).map_err(|e| io_err(&e))?;
            self.pages.insert(v, b);
        }
        Ok(self.pages.get_mut(&v).expect("inserted"))
    }

    /// Write the held pages out in runs.
    fn spill(&mut self) -> io::Result<()> {
        let pages = std::mem::take(&mut self.pages);
        let mut run: Vec<u8> = Vec::new();
        let mut start = 0;
        let mut next = None;
        for (v, b) in pages {
            if next != Some(v) && !run.is_empty() {
                self.store.ifs_write(start, &run).map_err(|e| io_err(&e))?;
                run.clear();
            }
            if run.is_empty() {
                start = v;
            }
            run.extend_from_slice(&b);
            next = Some(v + 1);
        }
        if !run.is_empty() {
            self.store.ifs_write(start, &run).map_err(|e| io_err(&e))?;
        }
        Ok(())
    }
}

impl Write for VolumeFile<'_> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if self.pos + buf.len() as u64 > self.len() {
            return Err(io::Error::other("past the end of the IFS volume"));
        }
        let mut done = 0;
        while done < buf.len() {
            let v = self.pos / PAGE_SIZE as u64;
            let off = usize::try_from(self.pos % PAGE_SIZE as u64).expect("small");
            let n = (PAGE_SIZE - off).min(buf.len() - done);
            if off == 0 && n == PAGE_SIZE {
                self.pages.insert(v, buf[done..done + n].to_vec());
            } else {
                self.page(v)?[off..off + n].copy_from_slice(&buf[done..done + n]);
            }
            done += n;
            self.pos += n as u64;
        }
        if self.pages.len() >= FILE_HELD {
            self.spill()?;
        }
        Ok(done)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.spill()?;
        self.store.ifs_flush().map_err(|e| io_err(&e))
    }
}

impl Read for VolumeFile<'_> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let left = self.len().saturating_sub(self.pos);
        let want = usize::try_from(left.min(buf.len() as u64)).expect("small");
        let mut done = 0;
        while done < want {
            let v = self.pos / PAGE_SIZE as u64;
            let off = usize::try_from(self.pos % PAGE_SIZE as u64).expect("small");
            let n = (PAGE_SIZE - off).min(want - done);
            let page = self.page(v)?;
            buf[done..done + n].copy_from_slice(&page[off..off + n]);
            done += n;
            self.pos += n as u64;
        }
        Ok(done)
    }
}

impl Seek for VolumeFile<'_> {
    fn seek(&mut self, to: SeekFrom) -> io::Result<u64> {
        let at = match to {
            SeekFrom::Start(n) => Some(n),
            SeekFrom::End(d) => self.len().checked_add_signed(d),
            SeekFrom::Current(d) => self.pos.checked_add_signed(d),
        };
        self.pos = at.ok_or_else(|| io::Error::other("seek before the start"))?;
        Ok(self.pos)
    }
}
