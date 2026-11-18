// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! [`StTransport`]: a drive through the Linux `st` driver.
//!
//! The driver's conventions (Documentation/scsi/st.rst and
//! drivers/scsi/st.c of Linux 6.18) this relies on:
//!
//! - A read returns one block's length; a read of 0 bytes is a file
//!   mark, unless MTIOCGET then reports `GMT_EOD` (the end of data). A
//!   block longer than the buffer fails with ENOMEM. EIO with `GMT_EOD`
//!   set is the end of data too.
//! - A volume never written answers its first read with BLANK CHECK,
//!   which the driver gives as EIO with neither `GMT_EOD` nor `GMT_BOT`
//!   (only a BLANK CHECK right after a file mark is noted as the end);
//!   so does data that ends without a file mark. So a read's EIO
//!   without `GMT_EOD` is put to the drive with `SG_IO`: BLANK CHECK is
//!   the end of data (at the start, a volume with no labels, and the
//!   drive is rewound); anything else stays a device error.
//! - Spacing that runs into the end of data or the beginning of tape
//!   fails with EIO; MTIOCGET's `GMT_EOD`, `GMT_BOT` and `GMT_EOF` tell which.
//! - The write that reaches the early warning is written and returns its
//!   full count. The next write fails with ENOSPC and writes nothing; the
//!   one after is let through, and past the early warning the two then
//!   alternate. So an ENOSPC is retried once: written then, the block is
//!   past the early warning; a second ENOSPC is the physical end (VOLUME
//!   OVERFLOW, nothing written). `GMT_EOT` is only up right after a
//!   write that saw the early warning, so it is not relied on.
//! - Writing file marks past the early warning succeeds (and resets the
//!   alternation); at the physical end it fails with EIO.
//! - With asynchronous writes the early warning of a write shows on the
//!   next call instead: a write, an MTIOCTOP or MTIOCGET then fails with
//!   ENOSPC having done nothing. Any call is retried once on ENOSPC.
//! - The driver writes a file mark itself before MTREW, MTOFFL, MTSEEK
//!   and MTBSF (or on close) after writes; every write here ends with
//!   MTWEOF, which clears that. When that mark fails (the physical end)
//!   the motion fails, so it is tried again after MTWEOF 0.
//! - EACCES on a write or MTWEOF is write protection (the node is open
//!   read-only, or the cartridge is protected); EROFS is the same at
//!   open or MTLOAD. ENOMEDIUM is no tape.
//! - After MTOFFL the driver refuses every call with ENOMEDIUM until
//!   MTLOAD (or a new open), so the next call after an unload loads.

use std::io;

use super::ioctl::{
    BLANK_CHECK, GMT_BOT, GMT_EOD, GMT_EOF, GMT_EOT, GMT_ONLINE, GMT_WR_PROT, MT_ST_BLKSIZE_MASK,
    MT_ST_DENSITY_SHIFT, MtGet, MtOp, StIoctl,
};
use crate::transport::{
    MAX_BLOCK, Position, ReadOutcome, Status, TapeTransport, TransportError, Written,
};

/// SCSI density codes and the IBM i format names they show as.
pub const DENSITIES: &[(u8, &str)] = &[
    (0x40, "*ULTRIUM1"),
    (0x42, "*ULTRIUM2"),
    (0x44, "*ULTRIUM3"),
    (0x46, "*ULTRIUM4"),
    (0x58, "*ULTRIUM5"),
    (0x5a, "*ULTRIUM6"),
    (0x5c, "*ULTRIUM7"),
    (0x5e, "*ULTRIUM8"),
    (0x60, "*ULTRIUM9"),
];

/// The format name for a density code.
#[must_use]
pub fn density_name(code: u8) -> String {
    if code == 0 {
        return "*DEVTYPE".into();
    }
    DENSITIES
        .iter()
        .find(|(c, _)| *c == code)
        .map_or_else(|| format!("X'{code:02X}'"), |(_, n)| (*n).to_string())
}

/// A tape drive behind the `st` driver.
#[derive(Debug)]
pub struct StTransport<I> {
    io: I,
    /// Past the early warning since the last motion.
    early_warning: bool,
    /// Unloaded here; the next call loads.
    unloaded: bool,
    /// The fixed block size set here, 0 for variable.
    block_size: u32,
    /// Rewound, and nothing read, written or spaced since: the next read
    /// is the volume's first.
    at_start: bool,
}

fn errno(e: &io::Error) -> Option<i32> {
    e.raw_os_error()
}

/// Linux errno values the driver returns.
mod errno {
    pub const EIO: i32 = 5;
    pub const ENOMEM: i32 = 12;
    pub const EACCES: i32 = 13;
    pub const ENOSPC: i32 = 28;
    pub const EROFS: i32 = 30;
    pub const ENOMEDIUM: i32 = 123;
}

fn map(e: io::Error) -> TransportError {
    match errno(&e) {
        Some(errno::ENOMEDIUM) => TransportError::NotLoaded,
        Some(errno::EROFS | errno::EACCES) => TransportError::WriteProtected,
        Some(errno::ENOSPC) => TransportError::EndOfMedium,
        _ => TransportError::Io(e),
    }
}

fn is(e: &io::Error, n: i32) -> bool {
    errno(e) == Some(n)
}

impl<I: StIoctl> StTransport<I> {
    /// Over an open `/dev/nstN` (the no-rewind node).
    pub fn new(io: I) -> Self {
        Self {
            io,
            early_warning: false,
            unloaded: false,
            block_size: 0,
            at_start: false,
        }
    }

    /// The ioctl backend.
    pub fn into_inner(self) -> I {
        self.io
    }

    /// The logical block address (MTIOCPOS).
    pub fn logical_block(&mut self) -> Result<u64, TransportError> {
        self.load()?;
        self.retried(StIoctl::mtpos).map_err(map)
    }

    /// One call, again once when it fails with ENOSPC: the early warning
    /// of an earlier write, or the driver's refusal of every other write
    /// past it. Both did nothing.
    fn retried<T>(&mut self, mut f: impl FnMut(&mut I) -> io::Result<T>) -> io::Result<T> {
        match f(&mut self.io) {
            Err(e) if is(&e, errno::ENOSPC) => {
                self.early_warning = true;
                f(&mut self.io)
            }
            r => r,
        }
    }

    /// MTLOAD after an unload here; EROFS is a loaded, protected volume.
    fn load(&mut self) -> Result<(), TransportError> {
        if !self.unloaded {
            return Ok(());
        }
        match self.io.mtop(MtOp::Load, 1) {
            Ok(()) => {}
            Err(e) if is(&e, errno::EROFS) || is(&e, errno::EACCES) => {}
            Err(e) if is(&e, errno::ENOMEDIUM) || is(&e, errno::EIO) => {
                return Err(TransportError::NotLoaded);
            }
            Err(e) => return Err(map(e)),
        }
        self.unloaded = false;
        self.at_start = true;
        self.early_warning = false;
        Ok(())
    }

    fn gstat(&mut self) -> Result<u64, TransportError> {
        Ok(self.retried(StIoctl::mtget).map_err(map)?.gstat)
    }

    /// An operation; EIO is explained by the status bits.
    fn op(&mut self, op: MtOp, count: u32) -> Result<(), TransportError> {
        self.load()?;
        let count = i32::try_from(count).unwrap_or(i32::MAX);
        match self.retried(|io| io.mtop(op, count)) {
            Ok(()) => Ok(()),
            Err(e) if is(&e, errno::EIO) => {
                let past = self.early_warning;
                let g = self.gstat()?;
                Err(if op == MtOp::Weof && (past || g & GMT_EOT != 0) {
                    TransportError::EndOfMedium
                } else if g & GMT_EOD != 0 {
                    TransportError::EndOfData
                } else if g & GMT_BOT != 0 && matches!(op, MtOp::Bsf | MtOp::Bsr) {
                    TransportError::BeginningOfTape
                } else if g & GMT_EOF != 0 && matches!(op, MtOp::Fsr | MtOp::Bsr) {
                    TransportError::TapeMark
                } else {
                    TransportError::Io(e)
                })
            }
            Err(e) => Err(map(e)),
        }
    }

    /// After a read's EIO the driver did not explain: whether the drive
    /// answers BLANK CHECK there, the end of data. The probe leaves the
    /// driver without a position; at the volume's first read it rewinds.
    fn blank_check(&mut self, first: bool) -> Result<bool, TransportError> {
        let sense = self.io.read_sense();
        if first {
            self.io.mtop(MtOp::Rew, 1).map_err(map)?;
        }
        Ok(matches!(sense, Ok(Some((BLANK_CHECK, _, _)))))
    }

    fn moved(&mut self, op: MtOp, count: u32) -> Result<(), TransportError> {
        self.at_start = false;
        let mut r = self.op(op, count);
        // After writes the driver writes a file mark before MTREW and
        // MTOFFL, which at the physical end fails, and keeps failing.
        // MTWEOF 0 (only a flush) ends the writing state; then again.
        if matches!(r, Err(TransportError::Io(_))) && matches!(op, MtOp::Rew | MtOp::Offl) {
            self.op(MtOp::Weof, 0)?;
            r = self.op(op, count);
        }
        self.early_warning = false;
        r
    }
}

impl<I: StIoctl> TapeTransport for StTransport<I> {
    fn rewind(&mut self) -> Result<(), TransportError> {
        self.moved(MtOp::Rew, 1)?;
        self.at_start = true;
        Ok(())
    }
    fn fsf(&mut self, n: u32) -> Result<(), TransportError> {
        self.moved(MtOp::Fsf, n)
    }
    fn bsf(&mut self, n: u32) -> Result<(), TransportError> {
        self.moved(MtOp::Bsf, n)
    }
    fn fsr(&mut self, n: u32) -> Result<(), TransportError> {
        self.moved(MtOp::Fsr, n)
    }
    fn bsr(&mut self, n: u32) -> Result<(), TransportError> {
        self.moved(MtOp::Bsr, n)
    }
    fn eom(&mut self) -> Result<(), TransportError> {
        self.moved(MtOp::Eom, 1)
    }
    fn write_eof(&mut self, n: u32) -> Result<(), TransportError> {
        self.at_start = false;
        self.op(MtOp::Weof, n)
    }
    fn erase(&mut self) -> Result<(), TransportError> {
        self.at_start = false;
        self.op(MtOp::Erase, 1)
    }
    fn unload(&mut self) -> Result<(), TransportError> {
        if self.unloaded {
            return Err(TransportError::NotLoaded);
        }
        self.moved(MtOp::Offl, 1)?;
        self.unloaded = true;
        Ok(())
    }
    fn set_block_size(&mut self, size: u32) -> Result<(), TransportError> {
        if size as usize > MAX_BLOCK {
            return Err(TransportError::BlockTooLarge {
                len: size as usize,
                max: MAX_BLOCK,
            });
        }
        self.op(MtOp::SetBlk, size)?;
        self.block_size = size;
        Ok(())
    }
    fn set_density(&mut self, density: &str) -> Result<(), TransportError> {
        let code = DENSITIES
            .iter()
            .find(|(_, n)| *n == density)
            .map(|(c, _)| *c)
            .ok_or_else(|| TransportError::Density(density.to_string()))?;
        self.op(MtOp::SetDensity, u32::from(code))
    }

    fn status(&mut self) -> Result<Status, TransportError> {
        match self.load() {
            Ok(()) | Err(TransportError::NotLoaded) => {}
            Err(e) => return Err(e),
        }
        let g: MtGet = self.retried(StIoctl::mtget).map_err(map)?;
        let density = u8::try_from((g.dsreg >> MT_ST_DENSITY_SHIFT) & 0xff).expect("masked");
        let as_u32 = |n: i32| u32::try_from(n).unwrap_or(0);
        Ok(Status {
            loaded: g.gstat & GMT_ONLINE != 0,
            bot: g.gstat & GMT_BOT != 0,
            eot: self.early_warning || g.gstat & GMT_EOT != 0,
            eod: g.gstat & GMT_EOD != 0,
            write_protected: g.gstat & GMT_WR_PROT != 0,
            density: density_name(density),
            block_size: u32::try_from(g.dsreg & MT_ST_BLKSIZE_MASK).expect("masked"),
            position: Position {
                file: as_u32(g.fileno),
                block: as_u32(g.blkno),
            },
        })
    }

    fn read_block(&mut self, buf: &mut [u8]) -> Result<ReadOutcome, TransportError> {
        self.load()?;
        // In fixed block mode the driver fills the count with as many
        // blocks as fit: ask for one.
        let n = match self.block_size as usize {
            0 => buf.len().min(MAX_BLOCK),
            bs if bs > buf.len() => {
                return Err(TransportError::BlockTooLarge {
                    len: bs,
                    max: buf.len(),
                });
            }
            bs => bs,
        };
        let first = std::mem::take(&mut self.at_start);
        match self.retried(|io| io.read(&mut buf[..n])) {
            Ok(0) => {
                // The physical end read shows as GMT_EOT: no more data.
                if self.gstat()? & (GMT_EOD | GMT_EOT) != 0 {
                    Ok(ReadOutcome::EndOfData)
                } else {
                    Ok(ReadOutcome::TapeMark)
                }
            }
            Ok(len) => Ok(ReadOutcome::Block(len)),
            Err(e) if is(&e, errno::ENOMEM) => {
                // The driver does not say how long the block was.
                Err(TransportError::BlockTooLarge { len: 0, max: n })
            }
            Err(e) if is(&e, errno::EIO) => {
                if self.gstat()? & GMT_EOD != 0 || self.blank_check(first)? {
                    Ok(ReadOutcome::EndOfData)
                } else {
                    Err(TransportError::Io(e))
                }
            }
            Err(e) => Err(map(e)),
        }
    }

    fn write_block(&mut self, data: &[u8]) -> Result<Written, TransportError> {
        self.at_start = false;
        if data.is_empty() || data.len() > MAX_BLOCK {
            return Err(TransportError::BlockTooLarge {
                len: data.len(),
                max: MAX_BLOCK,
            });
        }
        if self.block_size != 0 && data.len() != self.block_size as usize {
            return Err(TransportError::BlockSize {
                len: data.len(),
                size: self.block_size,
            });
        }
        self.load()?;
        match self.retried(|io| io.write(data)) {
            Ok(n) if n == data.len() => Ok(Written {
                early_warning: self.early_warning,
            }),
            // Part of a block went: the tape now holds a short block.
            Ok(n) => Err(TransportError::Io(io::Error::other(format!(
                "short write: {n} of {} bytes",
                data.len()
            )))),
            Err(e) if is(&e, errno::ENOSPC) => Err(TransportError::EndOfMedium),
            Err(e) => Err(map(e)),
        }
    }
}
