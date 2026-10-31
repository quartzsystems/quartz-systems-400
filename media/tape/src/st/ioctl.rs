// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The ioctls the transports need, as traits: the `st` driver's
//! MTIOCTOP, MTIOCGET and MTIOCPOS with read and write on its node, and
//! `SG_IO` on an `sg` node. The real calls are in `raw.rs`; tests replay
//! a transcript.

use std::io;

/// An MTIOCTOP operation (`linux/mtio.h`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(i16)]
pub enum MtOp {
    /// MTFSF: forward space over file marks.
    Fsf = 1,
    /// MTBSF: backward space over file marks.
    Bsf = 2,
    /// MTFSR: forward space records.
    Fsr = 3,
    /// MTBSR: backward space records.
    Bsr = 4,
    /// MTWEOF: write file marks.
    Weof = 5,
    /// MTREW: rewind.
    Rew = 6,
    /// MTOFFL: rewind and put the drive offline (unload).
    Offl = 7,
    /// MTEOM: space to the end of recorded data.
    Eom = 12,
    /// MTERASE: erase from the position.
    Erase = 13,
    /// MTSETBLK: set the block size (0 for variable).
    SetBlk = 20,
    /// MTSETDENSITY: set the density code.
    SetDensity = 21,
}

impl MtOp {
    /// The `mt_op` value.
    #[must_use]
    pub fn code(self) -> i16 {
        self as i16
    }

    /// The `linux/mtio.h` name, as transcripts write it.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Fsf => "MTFSF",
            Self::Bsf => "MTBSF",
            Self::Fsr => "MTFSR",
            Self::Bsr => "MTBSR",
            Self::Weof => "MTWEOF",
            Self::Rew => "MTREW",
            Self::Offl => "MTOFFL",
            Self::Eom => "MTEOM",
            Self::Erase => "MTERASE",
            Self::SetBlk => "MTSETBLK",
            Self::SetDensity => "MTSETDENSITY",
        }
    }

    /// Every operation.
    pub const ALL: [Self; 11] = [
        Self::Fsf,
        Self::Bsf,
        Self::Fsr,
        Self::Bsr,
        Self::Weof,
        Self::Rew,
        Self::Offl,
        Self::Eom,
        Self::Erase,
        Self::SetBlk,
        Self::SetDensity,
    ];
}

/// `mt_gstat` bit: at a file mark.
pub const GMT_EOF: u64 = 0x8000_0000;
/// `mt_gstat` bit: at the beginning of tape.
pub const GMT_BOT: u64 = 0x4000_0000;
/// `mt_gstat` bit: at the end of tape (early warning).
pub const GMT_EOT: u64 = 0x2000_0000;
/// `mt_gstat` bit: at a set mark.
pub const GMT_SM: u64 = 0x1000_0000;
/// `mt_gstat` bit: at the end of recorded data.
pub const GMT_EOD: u64 = 0x0800_0000;
/// `mt_gstat` bit: write protected.
pub const GMT_WR_PROT: u64 = 0x0400_0000;
/// `mt_gstat` bit: a tape is loaded and the drive is ready.
pub const GMT_ONLINE: u64 = 0x0100_0000;
/// `mt_gstat` bit: the door is open (no tape).
pub const GMT_DR_OPEN: u64 = 0x0004_0000;

/// `mt_dsreg`: block size mask.
pub const MT_ST_BLKSIZE_MASK: u64 = 0x00ff_ffff;
/// `mt_dsreg`: density code shift.
pub const MT_ST_DENSITY_SHIFT: u32 = 24;

/// What MTIOCGET returns (`struct mtget`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MtGet {
    /// Drive type.
    pub mt_type: i64,
    /// Residual count.
    pub resid: i64,
    /// Density code and block size.
    pub dsreg: u64,
    /// Generic status bits (`GMT_*`).
    pub gstat: u64,
    /// Error register.
    pub erreg: i64,
    /// File number, -1 when unknown.
    pub fileno: i32,
    /// Block number in the file, -1 when unknown.
    pub blkno: i32,
}

/// The `st` driver's calls on one open node.
pub trait StIoctl {
    /// MTIOCTOP.
    fn mtop(&mut self, op: MtOp, count: i32) -> io::Result<()>;
    /// MTIOCGET.
    fn mtget(&mut self) -> io::Result<MtGet>;
    /// MTIOCPOS: the logical block address.
    fn mtpos(&mut self) -> io::Result<u64>;
    /// `read` on the node: one block.
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize>;
    /// `write` on the node: one block.
    fn write(&mut self, data: &[u8]) -> io::Result<usize>;
}

/// `SG_IO` data direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SgDir {
    /// `SG_DXFER_NONE`.
    None,
    /// `SG_DXFER_TO_DEV`.
    ToDevice,
    /// `SG_DXFER_FROM_DEV`.
    FromDevice,
}

/// The outcome of one `SG_IO`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SgReply {
    /// SCSI status (0 good, 2 check condition).
    pub status: u8,
    /// Host adapter status.
    pub host_status: u16,
    /// Driver status.
    pub driver_status: u16,
    /// Sense data written.
    pub sense: Vec<u8>,
    /// Bytes not transferred.
    pub resid: i32,
}

/// `SG_IO` on an open `sg` node.
pub trait SgIoctl {
    /// Run one command. `data` is the transfer buffer.
    fn sg_io(
        &mut self,
        cdb: &[u8],
        dir: SgDir,
        data: &mut [u8],
        timeout_ms: u32,
    ) -> io::Result<SgReply>;
}
