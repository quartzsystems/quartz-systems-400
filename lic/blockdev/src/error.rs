// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Block device errors.

use std::io;
use std::path::PathBuf;

/// Result with a [`BlockError`].
pub type Result<T> = std::result::Result<T, BlockError>;

/// Why a block operation failed.
#[derive(Debug, thiserror::Error)]
pub enum BlockError {
    /// The device could not be opened.
    #[error("cannot open {path}: {source}")]
    Open {
        /// Path given to the backend.
        path: PathBuf,
        /// The OS error.
        #[source]
        source: io::Error,
    },

    /// An I/O operation failed at a block.
    #[error("{op} at LBA {lba}: {source}")]
    Io {
        /// `read`, `write`, `flush` or `discard`.
        op: &'static str,
        /// First block of the transfer.
        lba: u64,
        /// The OS error.
        #[source]
        source: io::Error,
    },

    /// A transfer was short: the OS moved fewer bytes than asked.
    #[error("short {op} at LBA {lba}: {done} of {len} bytes")]
    Short {
        /// `read` or `write`.
        op: &'static str,
        /// First block of the transfer.
        lba: u64,
        /// Bytes transferred.
        done: usize,
        /// Bytes requested.
        len: usize,
    },

    /// The buffer is empty or not a multiple of the block size.
    #[error("buffer of {len} bytes is not a whole number of blocks")]
    Unaligned {
        /// Buffer length.
        len: usize,
    },

    /// The transfer runs past the end of the device.
    #[error("LBA {lba} + {count} blocks is past the end of a {blocks}-block device")]
    OutOfRange {
        /// First block.
        lba: u64,
        /// Block count.
        count: u64,
        /// Device size in blocks.
        blocks: u64,
    },

    /// A fault the in-memory backend was told to inject.
    #[error("injected {kind} fault on write {write}")]
    Injected {
        /// `fail_after` or `tear_at`.
        kind: &'static str,
        /// Ordinal of the write that hit the fault (1-based).
        write: u64,
    },

    /// The device size is not a whole number of blocks or is zero.
    #[error("device size {bytes} bytes is not a positive multiple of the block size")]
    BadSize {
        /// Size reported by the OS.
        bytes: u64,
    },
}
