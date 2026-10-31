// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Labeled and unlabeled volumes over a [`TapeTransport`].
//!
//! A standard-labeled volume is
//!
//! ```text
//! VOL1 HDR1 HDR2 TM data... TM EOF1 EOF2 TM [HDR1 ... EOF2 TM]... TM
//! ```
//!
//! so the file at index `k` on a volume starts after `3k` tape marks and
//! the volume ends with two tape marks in a row. A file that reaches the
//! early warning point ends the volume with `TM EOV1 EOV2 TM TM` and
//! continues on the next volume after its `VOL1 HDR1 HDR2 TM`, with the
//! volume switch set in HDR2. An unlabeled (*NL) volume is files of data
//! blocks each ended by a tape mark, and one more tape mark at the end.

mod check;
mod copy;
mod dup;
mod inz;
mod provider;
mod read;
mod scan;
mod write;

pub use check::{ChkOptions, ChkResult, chktap};
pub use copy::{blocks_to_records, records_to_blocks};
pub use dup::{DupOptions, DupSeq, ToSeq, duptap};
pub use inz::{Check, InzOptions, inztap};
pub use provider::{LibraryProvider, NoMoreVolumes};
pub use read::{FileReader, ReadOptions};
pub use scan::{FileInfo, VolumeKind, VolumeLabels, identify, labels};
pub use write::{FileWriter, WriteOptions, WriteSummary};

use crate::TapeError;
use crate::transport::TapeTransport;

/// What to do with the volume when the operation ends.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EndOpt {
    /// ENDOPT(*REWIND).
    #[default]
    Rewind,
    /// ENDOPT(*LEAVE): stay where the operation stopped.
    Leave,
    /// ENDOPT(*UNLOAD): rewind and unload.
    Unload,
}

impl EndOpt {
    /// Apply it.
    pub fn apply<D: TapeTransport + ?Sized>(self, t: &mut D) -> Result<(), TapeError> {
        match self {
            Self::Rewind => t.rewind()?,
            Self::Leave => {}
            Self::Unload => t.unload()?,
        }
        Ok(())
    }
}

/// SEQNBR when writing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SeqNbr {
    /// A file sequence number, from 1.
    Number(u32),
    /// SEQNBR(*END): after the last file.
    End,
}

/// A request for the next volume of a multi-volume file. The caller
/// turns it into an inquiry to the system operator, or a changer move.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VolumeRequest {
    /// The volume sequence number wanted (2 for the first continuation).
    pub volume_seq: u32,
    /// The volume identifier from the VOL list, or `None` for any
    /// (VOL(*MOUNTED), or the list is used up).
    pub expected: Option<String>,
    /// The volume just finished.
    pub previous: String,
    /// The file label.
    pub file_label: String,
    /// The file sequence number.
    pub file_seq: u32,
    /// The file is being written (not read).
    pub writing: bool,
}

/// Mounts the next volume of a multi-volume file on the drive.
pub trait VolumeProvider<D: ?Sized> {
    /// Make the next volume ready on `drive`. Return
    /// [`TapeError::Cancelled`] when the operator ends the request.
    fn next_volume(&mut self, drive: &mut D, req: &VolumeRequest) -> Result<(), TapeError>;
}

/// Check a volume identifier (NEWVOL, VOL): 1 to 6 characters, letters,
/// digits and `$ # @`.
pub fn check_volume_id(param: &'static str, id: &str) -> Result<(), TapeError> {
    let ok = (1..=6).contains(&id.len())
        && id
            .bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b"$#@".contains(&b));
    if ok {
        Ok(())
    } else {
        Err(TapeError::Parameter {
            param,
            why: format!("{id:?} is not a volume identifier"),
        })
    }
}
