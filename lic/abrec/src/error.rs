// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Error type.

use std::io;

use thiserror::Error;

/// Anything the record code can fail with.
#[derive(Debug, Error)]
pub enum AbError {
    /// Underlying device I/O failed.
    #[error("record device I/O: {0}")]
    Io(#[from] io::Error),
    /// Neither copy holds a valid record.
    #[error("no valid record in either copy")]
    NoValidCopy,
    /// `init` was asked to overwrite a partition that still has a valid copy.
    #[error("the partition already holds a valid record; refusing to reinitialize")]
    AlreadyInitialized,
    /// The encoded record does not fit in one copy slot.
    #[error("record is {len} bytes, slot holds {max}")]
    TooLarge {
        /// Encoded size.
        len: usize,
        /// Slot capacity.
        max: usize,
    },
    /// A copy failed to decode.
    #[error("malformed record: {0}")]
    Malformed(&'static str),
    /// No partition with the record's type GUID exists.
    #[error("no record partition found by GPT type GUID")]
    NotFound,
}
