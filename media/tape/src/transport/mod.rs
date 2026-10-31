// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The drive and changer traits every layer above talks to.
//!
//! Positioning follows the `st` driver: forward space file leaves the
//! drive just past the tape mark, backward space file just before it
//! (on its beginning-of-tape side). Reading a tape mark consumes it.

mod changer;

pub use changer::{Changer, ChangerError, Element, ElementKind};

/// The largest block a transport reads or writes: 256 KiB.
pub const MAX_BLOCK: usize = 256 * 1024;

/// Where the drive is: tape marks passed since the beginning of tape, and
/// blocks since the last of them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Position {
    /// File number (tape marks before the position).
    pub file: u32,
    /// Block number within the file.
    pub block: u32,
}

/// What a drive reports. The flags mirror the drive's status bits.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Status {
    /// A volume is loaded and ready.
    pub loaded: bool,
    /// At the beginning of tape.
    pub bot: bool,
    /// Past the early warning point near the end of the medium.
    pub eot: bool,
    /// At the end of recorded data.
    pub eod: bool,
    /// The volume is write protected.
    pub write_protected: bool,
    /// The density or format, as the drive names it.
    pub density: String,
    /// Fixed block size, 0 for variable blocks.
    pub block_size: u32,
    /// The position.
    pub position: Position,
}

/// What one block read found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadOutcome {
    /// A data block of this length.
    Block(usize),
    /// A tape mark; the drive is now past it.
    TapeMark,
    /// The end of recorded data; the drive did not move.
    EndOfData,
}

/// A block was recorded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Written {
    /// The write ended past the early warning point: finish the volume.
    pub early_warning: bool,
}

/// A drive failed an operation.
#[derive(Debug, thiserror::Error)]
pub enum TransportError {
    /// No volume is loaded.
    #[error("no volume is loaded")]
    NotLoaded,
    /// The volume is write protected.
    #[error("the volume is write protected")]
    WriteProtected,
    /// No room for the block: the physical end of the medium.
    #[error("end of medium reached; the block was not written")]
    EndOfMedium,
    /// Spacing ran into the beginning of tape.
    #[error("beginning of tape reached while spacing")]
    BeginningOfTape,
    /// Spacing or reading ran into the end of recorded data.
    #[error("end of recorded data reached while spacing")]
    EndOfData,
    /// Spacing over blocks ran into a tape mark (the drive stops past it).
    #[error("tape mark reached while spacing over blocks")]
    TapeMark,
    /// A block longer than the transport supports, or than the buffer.
    #[error("block of {len} bytes does not fit {max}")]
    BlockTooLarge {
        /// Block length.
        len: usize,
        /// The limit it broke.
        max: usize,
    },
    /// A write that does not match the fixed block size.
    #[error("block of {len} bytes in fixed {size}-byte block mode")]
    BlockSize {
        /// Block length.
        len: usize,
        /// The fixed block size.
        size: u32,
    },
    /// A density the drive does not know.
    #[error("density {0} is not supported by this drive")]
    Density(String),
    /// A virtual volume file is not a volume.
    #[error("not a virtual tape volume: {0}")]
    Format(String),
    /// The device failed.
    #[error("device error: {0}")]
    Io(#[from] std::io::Error),
}

/// One tape drive with a volume mounted (or not).
pub trait TapeTransport {
    /// Rewind to the beginning of tape.
    fn rewind(&mut self) -> Result<(), TransportError>;
    /// Space forward over `n` tape marks, stopping just past the last.
    fn fsf(&mut self, n: u32) -> Result<(), TransportError>;
    /// Space backward over `n` tape marks, stopping just before the last.
    fn bsf(&mut self, n: u32) -> Result<(), TransportError>;
    /// Space forward over `n` blocks.
    fn fsr(&mut self, n: u32) -> Result<(), TransportError>;
    /// Space backward over `n` blocks.
    fn bsr(&mut self, n: u32) -> Result<(), TransportError>;
    /// Space to the end of recorded data.
    fn eom(&mut self) -> Result<(), TransportError>;
    /// Write `n` tape marks at the position; everything after is lost.
    fn write_eof(&mut self, n: u32) -> Result<(), TransportError>;
    /// Erase from the position to the end of the medium.
    fn erase(&mut self) -> Result<(), TransportError>;
    /// Rewind and take the volume offline (unload).
    fn unload(&mut self) -> Result<(), TransportError>;
    /// Fixed block size, or 0 for variable blocks.
    fn set_block_size(&mut self, size: u32) -> Result<(), TransportError>;
    /// Change the density or format for the next write from the beginning.
    fn set_density(&mut self, density: &str) -> Result<(), TransportError>;
    /// What the drive reports.
    fn status(&mut self) -> Result<Status, TransportError>;
    /// Read one block into `buf` (at most [`MAX_BLOCK`] bytes).
    fn read_block(&mut self, buf: &mut [u8]) -> Result<ReadOutcome, TransportError>;
    /// Write one block at the position; everything after is lost.
    fn write_block(&mut self, data: &[u8]) -> Result<Written, TransportError>;
}
