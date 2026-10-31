// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! [`StTransport`]: a drive through the Linux `st` driver.
//!
//! The driver's conventions (Documentation/scsi/st.rst) this relies on:
//!
//! - A read returns one block's length; a read of 0 bytes is a file
//!   mark, unless MTIOCGET then reports `GMT_EOD` (the end of data). A
//!   block longer than the buffer fails with ENOMEM. EIO with `GMT_EOD`
//!   set is the end of data too.
//! - Spacing that runs into the end of data or the beginning of tape
//!   fails with EIO; MTIOCGET's `GMT_EOD`, `GMT_BOT` and `GMT_EOF` tell which.
//! - At the early warning a write fails with ENOSPC and MTIOCGET shows
//!   `GMT_EOT`; the driver then accepts writes up to the physical end,
//!   so the block is written again once and reported with the early
//!   warning. A second ENOSPC (or EIO) is the end of the medium.
//! - EROFS or EACCES on a write is write protection; ENOMEDIUM is no
//!   tape.

use std::io;

use super::ioctl::{
    GMT_BOT, GMT_EOD, GMT_EOF, GMT_EOT, GMT_ONLINE, GMT_WR_PROT, MT_ST_BLKSIZE_MASK,
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
    early_warning: bool,
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

impl<I: StIoctl> StTransport<I> {
    /// Over an open `/dev/nstN` (the no-rewind node).
    pub fn new(io: I) -> Self {
        Self {
            io,
            early_warning: false,
        }
    }

    /// The ioctl backend.
    pub fn into_inner(self) -> I {
        self.io
    }

    /// The logical block address (MTIOCPOS).
    pub fn logical_block(&mut self) -> Result<u64, TransportError> {
        self.io.mtpos().map_err(map)
    }

    fn gstat(&mut self) -> Result<u64, TransportError> {
        Ok(self.io.mtget()?.gstat)
    }

    /// An operation; EIO is explained by the status bits.
    fn op(&mut self, op: MtOp, count: u32) -> Result<(), TransportError> {
        let count = i32::try_from(count).unwrap_or(i32::MAX);
        match self.io.mtop(op, count) {
            Ok(()) => Ok(()),
            Err(e) if errno(&e) == Some(errno::EIO) => {
                let g = self.gstat()?;
                Err(if g & GMT_EOD != 0 {
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

    fn moved(&mut self, op: MtOp, count: u32) -> Result<(), TransportError> {
        self.early_warning = false;
        self.op(op, count)
    }
}

impl<I: StIoctl> TapeTransport for StTransport<I> {
    fn rewind(&mut self) -> Result<(), TransportError> {
        self.moved(MtOp::Rew, 1)
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
        self.op(MtOp::Weof, n)
    }
    fn erase(&mut self) -> Result<(), TransportError> {
        self.op(MtOp::Erase, 1)
    }
    fn unload(&mut self) -> Result<(), TransportError> {
        self.moved(MtOp::Offl, 1)
    }
    fn set_block_size(&mut self, size: u32) -> Result<(), TransportError> {
        if size as usize > MAX_BLOCK {
            return Err(TransportError::BlockTooLarge {
                len: size as usize,
                max: MAX_BLOCK,
            });
        }
        self.op(MtOp::SetBlk, size)
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
        let g: MtGet = self.io.mtget().map_err(map)?;
        let density = u8::try_from((g.dsreg >> MT_ST_DENSITY_SHIFT) & 0xff).expect("masked");
        let as_u32 = |n: i32| u32::try_from(n).unwrap_or(0);
        Ok(Status {
            loaded: g.gstat & GMT_ONLINE != 0,
            bot: g.gstat & GMT_BOT != 0,
            eot: g.gstat & GMT_EOT != 0,
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
        let n = buf.len().min(MAX_BLOCK);
        match self.io.read(&mut buf[..n]) {
            Ok(0) => {
                if self.gstat()? & GMT_EOD != 0 {
                    Ok(ReadOutcome::EndOfData)
                } else {
                    Ok(ReadOutcome::TapeMark)
                }
            }
            Ok(len) => Ok(ReadOutcome::Block(len)),
            Err(e) if errno(&e) == Some(errno::ENOMEM) => {
                // The driver does not say how long the block was.
                Err(TransportError::BlockTooLarge { len: 0, max: n })
            }
            Err(e) if errno(&e) == Some(errno::EIO) => {
                if self.gstat()? & GMT_EOD != 0 {
                    Ok(ReadOutcome::EndOfData)
                } else {
                    Err(TransportError::Io(e))
                }
            }
            Err(e) => Err(map(e)),
        }
    }

    fn write_block(&mut self, data: &[u8]) -> Result<Written, TransportError> {
        if data.is_empty() || data.len() > MAX_BLOCK {
            return Err(TransportError::BlockTooLarge {
                len: data.len(),
                max: MAX_BLOCK,
            });
        }
        let short = |n: usize| {
            TransportError::Io(io::Error::other(format!(
                "short write: {n} of {} bytes",
                data.len()
            )))
        };
        match self.io.write(data) {
            Ok(n) if n == data.len() => Ok(Written {
                early_warning: self.early_warning,
            }),
            Ok(n) => Err(short(n)),
            Err(e) if errno(&e) == Some(errno::ENOSPC) && !self.early_warning => {
                if self.gstat()? & GMT_EOT == 0 {
                    return Err(TransportError::EndOfMedium);
                }
                self.early_warning = true;
                match self.io.write(data) {
                    Ok(n) if n == data.len() => Ok(Written {
                        early_warning: true,
                    }),
                    Ok(n) => Err(short(n)),
                    Err(e) if matches!(errno(&e), Some(errno::ENOSPC | errno::EIO)) => {
                        Err(TransportError::EndOfMedium)
                    }
                    Err(e) => Err(map(e)),
                }
            }
            Err(e) if errno(&e) == Some(errno::EIO) && self.early_warning => {
                Err(TransportError::EndOfMedium)
            }
            Err(e) => Err(map(e)),
        }
    }
}
