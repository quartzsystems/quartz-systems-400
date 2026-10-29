// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Errors of the formatter.

use std::io;

/// Why a filesystem could not be laid out, written or read.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The device is smaller than the smallest filesystem.
    #[error("device of {bytes} bytes is smaller than the {min}-byte minimum")]
    TooSmall {
        /// Size offered.
        bytes: u64,
        /// Size needed.
        min: u64,
    },
    /// The device is larger than this formatter lays out.
    #[error("device of {bytes} bytes is larger than the {max}-byte maximum")]
    TooLarge {
        /// Size offered.
        bytes: u64,
        /// Largest size.
        max: u64,
    },
    /// A sector size other than 512 or 4096.
    #[error("unsupported sector size {0}")]
    SectorSize(u32),
    /// The label does not fit the 12-byte field.
    #[error("label of {0} bytes is longer than 12")]
    Label(usize),
    /// An I/O error on the device.
    #[error("{what}: {source}")]
    Io {
        /// What was being done.
        what: &'static str,
        /// The OS error.
        #[source]
        source: io::Error,
    },
}

impl Error {
    /// Wrap an I/O error.
    pub(crate) fn io(what: &'static str) -> impl FnOnce(io::Error) -> Self {
        move |source| Self::Io { what, source }
    }
}
