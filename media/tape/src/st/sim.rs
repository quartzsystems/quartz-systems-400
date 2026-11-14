// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! [`StSim`]: the `st` driver's answers over cartridges in memory, for
//! tests of [`StTransport`](super::StTransport) and the layers above it.
//!
//! It follows drivers/scsi/st.c of Linux 6.18 over an SSC drive: the
//! `eof` state machine of `st_write`, `st_read` and `st_int_ioctl`
//! (`ST_NOEOF`, `ST_FM`, `ST_EOM_OK`, `ST_EOD_1`, `ST_EOD_2`, `ST_EOD`),
//! the early warning reported on the write that reaches it, the
//! ENOSPC on every other write past it, VOLUME OVERFLOW at the physical
//! end, asynchronous writes (the early warning shows on the next call),
//! the file mark written before MTREW, MTOFFL and MTBSF after writes,
//! write protection (EROFS at MTLOAD on a read-write node, EACCES on a
//! write) and ENOMEDIUM after MTOFFL until MTLOAD. Every call is logged
//! as `CALL -> RESULT`.

use std::collections::VecDeque;
use std::io;

use super::ioctl::{
    GMT_BOT, GMT_DR_OPEN, GMT_EOD, GMT_EOF, GMT_EOT, GMT_ONLINE, GMT_WR_PROT, MT_ST_DENSITY_SHIFT,
    MtGet, MtOp, StIoctl,
};

/// What a file mark takes of the medium, in bytes.
pub const MARK_LEN: u64 = 512;

/// One record on a cartridge.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SimRecord {
    /// A data block.
    Block(Vec<u8>),
    /// A file mark.
    Mark,
}

impl SimRecord {
    fn len(&self) -> u64 {
        match self {
            Self::Block(b) => b.len() as u64,
            Self::Mark => MARK_LEN,
        }
    }
}

/// A cartridge.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SimCartridge {
    /// A name for the test's messages.
    pub name: String,
    /// What is recorded.
    pub records: Vec<SimRecord>,
    /// Bytes to the early warning point.
    pub early_warning: u64,
    /// Bytes to the physical end.
    pub capacity: u64,
    /// The write-protect tab.
    pub write_protected: bool,
}

impl SimCartridge {
    /// A blank cartridge.
    #[must_use]
    pub fn blank(name: &str, early_warning: u64, capacity: u64) -> Self {
        Self {
            name: name.into(),
            records: Vec::new(),
            early_warning,
            capacity,
            write_protected: false,
        }
    }

    /// Bytes used by the first `n` records.
    fn used(&self, n: usize) -> u64 {
        self.records[..n].iter().map(SimRecord::len).sum()
    }
}

/// The driver's `eof` state (st.h; `Clear` is `ST_NOEOF`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Eof {
    Clear,
    Fm,
    EomOk,
    Eod1,
    Eod2,
    Eod,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Rw {
    Idle,
    Reading,
    Writing,
}

/// A drive behind the `st` driver, with an open no-rewind node.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug)]
pub struct StSim {
    /// Every cartridge, the one in the drive included.
    pub cartridges: Vec<SimCartridge>,
    /// The cartridge in the drive, by index.
    pub in_drive: Option<usize>,
    /// The cartridges MTLOAD takes next, in order (an autoloader, or
    /// the operator).
    pub queue: VecDeque<usize>,
    /// The calls made, `CALL -> RESULT`.
    pub log: Vec<String>,
    /// `MT_ST_ASYNC_WRITES` in effect (variable blocks without direct
    /// I/O): a write returns before the drive answers.
    pub async_writes: bool,
    read_write: bool,
    write_prot: bool,
    ready: bool,
    pos: usize,
    eof: Eof,
    rw: Rw,
    block_size: u32,
    pending_ew: bool,
    blkno_unknown: bool,
}

fn err(n: i32) -> io::Error {
    io::Error::from_raw_os_error(n)
}

const EIO: i32 = 5;
const ENOMEM: i32 = 12;
const EACCES: i32 = 13;
const EINVAL: i32 = 22;
const ENOSPC: i32 = 28;
const EROFS: i32 = 30;
const ENOMEDIUM: i32 = 123;

fn name(r: &io::Result<impl std::fmt::Debug>) -> String {
    match r {
        Ok(v) => format!("{v:?}"),
        Err(e) => match e.raw_os_error() {
            Some(EIO) => "EIO".into(),
            Some(ENOMEM) => "ENOMEM".into(),
            Some(EACCES) => "EACCES".into(),
            Some(EINVAL) => "EINVAL".into(),
            Some(ENOSPC) => "ENOSPC".into(),
            Some(EROFS) => "EROFS".into(),
            Some(ENOMEDIUM) => "ENOMEDIUM".into(),
            _ => e.to_string(),
        },
    }
}

impl StSim {
    /// The node opened on `cartridges[0]` in the drive (the others
    /// queued), read-write when `read_write` (as `LinuxSt::open` does
    /// unless the cartridge is protected).
    #[must_use]
    pub fn new(cartridges: Vec<SimCartridge>, read_write: bool) -> Self {
        let queue = (1..cartridges.len()).collect();
        let protected = cartridges.first().is_some_and(|c| c.write_protected);
        Self {
            in_drive: (!cartridges.is_empty()).then_some(0),
            ready: !cartridges.is_empty(),
            cartridges,
            queue,
            log: Vec::new(),
            async_writes: false,
            read_write,
            write_prot: !read_write || protected,
            pos: 0,
            eof: Eof::Clear,
            rw: Rw::Idle,
            block_size: 0,
            pending_ew: false,
            blkno_unknown: false,
        }
    }

    /// The log lines that start with `prefix`.
    #[must_use]
    pub fn calls(&self, prefix: &str) -> usize {
        self.log.iter().filter(|l| l.starts_with(prefix)).count()
    }

    fn cart(&self) -> &SimCartridge {
        &self.cartridges[self.in_drive.expect("a cartridge")]
    }

    fn cart_mut(&mut self) -> &mut SimCartridge {
        let i = self.in_drive.expect("a cartridge");
        &mut self.cartridges[i]
    }

    fn end(&self) -> usize {
        self.cart().records.len()
    }

    /// Record at the position; what follows is gone (as on tape).
    fn put(&mut self, r: SimRecord) -> bool {
        let pos = self.pos;
        let c = self.cart_mut();
        c.records.truncate(pos);
        if c.used(pos) + r.len() > c.capacity {
            return false;
        }
        c.records.push(r);
        self.pos += 1;
        true
    }

    fn past_ew(&self) -> bool {
        self.cart().used(self.pos) > self.cart().early_warning
    }

    /// `flush_buffer`: an asynchronous write's early warning.
    fn flush(&mut self) -> io::Result<()> {
        if self.pending_ew {
            self.pending_ew = false;
            return Err(err(ENOSPC));
        }
        Ok(())
    }

    fn weof(&mut self, n: i32) -> io::Result<()> {
        if self.write_prot {
            return Err(err(EACCES));
        }
        for _ in 0..n {
            if !self.put(SimRecord::Mark) {
                self.eof = Eof::EomOk;
                return Err(err(EIO));
            }
        }
        self.eof = Eof::Clear;
        self.rw = Rw::Idle;
        Ok(())
    }

    fn load(&mut self) -> io::Result<()> {
        if self.in_drive.is_none() {
            self.in_drive = self.queue.pop_front();
        }
        if self.in_drive.is_none() {
            return Err(err(EIO));
        }
        self.ready = true;
        self.pos = 0;
        self.eof = Eof::Clear;
        self.rw = Rw::Idle;
        self.block_size = 0;
        self.pending_ew = false;
        self.blkno_unknown = false;
        self.write_prot = !self.read_write;
        if self.cart().write_protected {
            self.write_prot = true;
            if self.read_write {
                return Err(err(EROFS));
            }
        }
        Ok(())
    }

    fn space(&mut self, op: MtOp, count: i32) -> io::Result<()> {
        let n = usize::try_from(count).unwrap_or(0);
        match op {
            MtOp::Fsf => {
                self.blkno_unknown = false;
                for _ in 0..n {
                    let recs = &self.cart().records;
                    let Some(i) = recs[self.pos..].iter().position(|r| *r == SimRecord::Mark)
                    else {
                        self.pos = recs.len();
                        self.eof = Eof::Eod;
                        return Err(err(EIO));
                    };
                    self.pos += i + 1;
                }
                self.eof = Eof::Fm;
            }
            MtOp::Bsf => {
                for _ in 0..n {
                    let recs = &self.cart().records;
                    let Some(i) = recs[..self.pos].iter().rposition(|r| *r == SimRecord::Mark)
                    else {
                        self.pos = 0;
                        self.blkno_unknown = false;
                        self.eof = Eof::Clear;
                        return Err(err(EIO));
                    };
                    self.pos = i;
                }
                self.blkno_unknown = true;
                self.eof = Eof::Clear;
            }
            MtOp::Fsr => {
                for _ in 0..n {
                    match self.cart().records.get(self.pos) {
                        None => {
                            self.eof = Eof::Eod;
                            return Err(err(EIO));
                        }
                        Some(SimRecord::Mark) => {
                            self.pos += 1;
                            self.eof = Eof::Fm;
                            return Err(err(EIO));
                        }
                        Some(SimRecord::Block(_)) => self.pos += 1,
                    }
                }
                self.eof = Eof::Clear;
            }
            MtOp::Bsr => {
                for _ in 0..n {
                    if self.pos == 0 {
                        return Err(err(EIO));
                    }
                    if self.cart().records[self.pos - 1] == SimRecord::Mark {
                        self.pos -= 1;
                        self.blkno_unknown = true;
                        return Err(err(EIO));
                    }
                    self.pos -= 1;
                }
                self.eof = Eof::Clear;
            }
            _ => unreachable!("not a spacing operation"),
        }
        Ok(())
    }

    fn do_mtop(&mut self, op: MtOp, mut count: i32) -> io::Result<()> {
        if op == MtOp::Load {
            return self.load();
        }
        if !self.ready {
            return Err(err(ENOMEDIUM));
        }
        self.flush()?;
        if self.rw == Rw::Writing && matches!(op, MtOp::Rew | MtOp::Offl | MtOp::Bsf) {
            self.weof(1)?;
            if op == MtOp::Bsf {
                count += 1;
            }
        }
        if !matches!(op, MtOp::SetBlk | MtOp::SetDensity) {
            self.rw = Rw::Idle;
        }
        match op {
            MtOp::Weof => self.weof(count)?,
            MtOp::Rew => {
                self.pos = 0;
                self.eof = Eof::Clear;
                self.blkno_unknown = false;
            }
            MtOp::Offl => {
                self.ready = false;
                self.pos = 0;
                self.in_drive = None;
            }
            MtOp::Fsf | MtOp::Bsf | MtOp::Fsr | MtOp::Bsr => self.space(op, count)?,
            MtOp::Eom => {
                self.pos = self.end();
                self.eof = Eof::Eod;
                self.blkno_unknown = false;
            }
            MtOp::Erase => {
                if self.write_prot {
                    return Err(err(EACCES));
                }
                let pos = self.pos;
                self.cart_mut().records.truncate(pos);
            }
            MtOp::SetBlk => self.block_size = u32::try_from(count).unwrap_or(0),
            MtOp::SetDensity | MtOp::Load => {}
        }
        Ok(())
    }

    fn do_mtget(&mut self) -> io::Result<MtGet> {
        if !self.ready {
            return Ok(MtGet {
                gstat: GMT_DR_OPEN,
                fileno: -1,
                blkno: -1,
                ..MtGet::default()
            });
        }
        self.flush()?;
        let recs = &self.cart().records[..self.pos];
        let fileno = recs.iter().filter(|r| **r == SimRecord::Mark).count();
        let last = recs.iter().rposition(|r| *r == SimRecord::Mark);
        let blkno = if self.blkno_unknown {
            -1
        } else {
            i32::try_from(self.pos - last.map_or(0, |i| i + 1)).unwrap_or(i32::MAX)
        };
        let fileno = i32::try_from(fileno).unwrap_or(i32::MAX);
        let mut gstat = GMT_ONLINE;
        if self.cart().write_protected {
            gstat |= GMT_WR_PROT;
        }
        if blkno == 0 {
            gstat |= if fileno == 0 { GMT_BOT } else { GMT_EOF };
        }
        if self.eof == Eof::EomOk {
            gstat |= GMT_EOT;
        } else if self.eof >= Eof::EomOk {
            gstat |= GMT_EOD;
        }
        Ok(MtGet {
            mt_type: 0x72,
            dsreg: (0x58 << MT_ST_DENSITY_SHIFT) | u64::from(self.block_size),
            gstat,
            fileno,
            blkno,
            ..MtGet::default()
        })
    }

    fn do_read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if !self.ready {
            return Err(err(ENOMEDIUM));
        }
        if self.rw == Rw::Writing {
            self.flush()?;
        }
        if self.eof >= Eof::Eod1 {
            if self.eof < Eof::Eod {
                self.eof = if self.eof == Eof::Eod1 {
                    Eof::Eod2
                } else {
                    Eof::Eod
                };
                return Ok(0);
            }
            return Err(err(EIO));
        }
        self.rw = Rw::Reading;
        let bs = self.block_size as usize;
        if bs != 0 {
            // Fixed blocks: as many as the count holds, up to a mark.
            let mut got = 0;
            while got + bs <= buf.len() {
                match self.cart().records.get(self.pos) {
                    Some(SimRecord::Block(b)) if b.len() == bs => {
                        buf[got..got + bs].copy_from_slice(b);
                        got += bs;
                        self.pos += 1;
                    }
                    Some(SimRecord::Block(_)) if got == 0 => return Err(err(EIO)),
                    _ => break,
                }
            }
            if got > 0 {
                self.eof = Eof::Clear;
                return Ok(got);
            }
        }
        let Some(rec) = self.cart().records.get(self.pos).cloned() else {
            // BLANK CHECK: 0 bytes the first time after a file mark.
            let after_mark = self.eof == Eof::Fm;
            self.eof = Eof::Eod;
            return if after_mark { Ok(0) } else { Err(err(EIO)) };
        };
        self.pos += 1;
        match rec {
            SimRecord::Mark => {
                self.eof = Eof::Fm;
                Ok(0)
            }
            SimRecord::Block(b) if b.len() > buf.len() => Err(err(ENOMEM)),
            SimRecord::Block(b) => {
                buf[..b.len()].copy_from_slice(&b);
                self.eof = Eof::Clear;
                Ok(b.len())
            }
        }
    }

    fn do_write(&mut self, data: &[u8]) -> io::Result<usize> {
        if !self.ready {
            return Err(err(ENOMEDIUM));
        }
        let bs = self.block_size as usize;
        if bs != 0 && !data.len().is_multiple_of(bs) {
            return Err(err(EINVAL));
        }
        if self.write_prot {
            return Err(err(EACCES));
        }
        if self.pending_ew {
            self.pending_ew = false;
            self.eof = Eof::EomOk;
        }
        if self.eof == Eof::EomOk {
            self.eof = Eof::Eod1;
            return Err(err(ENOSPC));
        }
        let asynchronous = bs == 0 && self.async_writes && self.eof < Eof::EomOk;
        self.rw = Rw::Writing;
        let blocks: Vec<&[u8]> = if bs == 0 {
            vec![data]
        } else {
            data.chunks(bs).collect()
        };
        let mut written = 0;
        for b in blocks {
            if !self.put(SimRecord::Block(b.to_vec())) {
                // VOLUME OVERFLOW: the rest is not written.
                self.eof = Eof::EomOk;
                return if written == 0 {
                    Err(err(ENOSPC))
                } else {
                    Ok(written)
                };
            }
            written += b.len();
        }
        if self.past_ew() {
            if asynchronous {
                self.pending_ew = true;
                self.eof = Eof::Clear;
            } else {
                self.eof = Eof::EomOk;
            }
        } else if self.eof == Eof::Eod1 {
            self.eof = Eof::EomOk;
        } else {
            self.eof = Eof::Clear;
        }
        Ok(written)
    }
}

impl StIoctl for StSim {
    fn mtop(&mut self, op: MtOp, count: i32) -> io::Result<()> {
        let r = self.do_mtop(op, count);
        self.log
            .push(format!("mtop {} {count} -> {}", op.name(), name(&r)));
        r
    }

    fn mtget(&mut self) -> io::Result<MtGet> {
        let r = self.do_mtget();
        let s = match &r {
            Ok(g) => format!("gstat={:#x}", g.gstat),
            Err(_) => name(&r),
        };
        self.log.push(format!("mtget -> {s}"));
        r
    }

    fn mtpos(&mut self) -> io::Result<u64> {
        let r = if self.ready {
            Ok(self.pos as u64)
        } else {
            Err(err(ENOMEDIUM))
        };
        self.log.push(format!("mtpos -> {}", name(&r)));
        r
    }

    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let r = self.do_read(buf);
        self.log.push(format!("read {} -> {}", buf.len(), name(&r)));
        r
    }

    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        let r = self.do_write(data);
        self.log
            .push(format!("write {} -> {}", data.len(), name(&r)));
        r
    }
}
