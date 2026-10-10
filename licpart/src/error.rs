// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Error type.

use std::io;

use thiserror::Error;

/// Anything the LIC partition code can fail with.
#[derive(Debug, Error)]
pub enum LicError {
    /// Underlying device I/O failed.
    #[error("LIC device I/O: {0}")]
    Io(#[from] io::Error),
    /// Neither copy holds a valid record.
    #[error("no valid LIC record in either copy")]
    NoValidCopy,
    /// `init` was asked to overwrite a partition that still has a valid copy.
    #[error("LIC partition already holds a valid record; refusing to reinitialize")]
    AlreadyInitialized,
    /// The encoded record does not fit in one copy slot.
    #[error("LIC record is {len} bytes, slot holds {max}")]
    TooLarge {
        /// Encoded size.
        len: usize,
        /// Slot capacity.
        max: usize,
    },
    /// A record failed to decode.
    #[error("malformed LIC record: {0}")]
    Malformed(&'static str),
    /// A TLV tag below `TAG_USER_MIN` was used from outside the crate.
    #[error("TLV tag {0:#06x} is reserved")]
    ReservedTag(u16),
    /// No partition with the LIC type GUID exists.
    #[error("no LIC partition found by GPT type GUID")]
    NotFound,
}
