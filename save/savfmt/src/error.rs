// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Save stream errors.

/// What went wrong reading or writing a stream.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The medium failed.
    #[error("save medium: {0}")]
    Io(#[from] std::io::Error),
    /// Not a QS/400 save stream.
    #[error("not a QS/400 save stream")]
    NotSave,
    /// A format version this release does not read.
    #[error("save format version {0} is not supported")]
    Version(u16),
    /// The media header's checksum is wrong.
    #[error("save media header is damaged")]
    HeaderDamaged,
    /// A record header is damaged or out of sequence at `offset`.
    #[error("save stream damaged at offset {offset}")]
    Damaged {
        /// Stream offset of the bad record.
        offset: u64,
    },
    /// A record's payload failed its checksum.
    #[error("record {seq} of the save stream failed its checksum")]
    BadCrc {
        /// The record number.
        seq: u32,
    },
    /// The stream ends before its trailer.
    #[error("save stream is incomplete")]
    Truncated,
    /// An object's parts do not add up or its digest is wrong.
    #[error("saved object {index} is damaged")]
    ObjectDamaged {
        /// The object's index in the stream.
        index: u32,
    },
    /// A compressed chunk did not decompress to its length.
    #[error("compressed data is damaged")]
    Codec,
    /// The writer was used out of order.
    #[error("save stream misuse: {0}")]
    Misuse(&'static str),
}

/// Save stream result.
pub type Result<T> = std::result::Result<T, Error>;
