// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The crate's errors and the message identifiers a command layer shows
//! for them.

/// Result alias.
pub type Result<T> = std::result::Result<T, Error>;

/// What went wrong.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The image or device could not be read or written.
    #[error("optical I/O failed: {0}")]
    Io(#[from] std::io::Error),
    /// The image is not well formed.
    #[error("the optical image is damaged: {0}")]
    Malformed(String),
    /// No such file or directory.
    #[error("{0} is not on the volume")]
    NotFound(String),
    /// The name is taken.
    #[error("{0} is already on the volume")]
    Exists(String),
    /// A path component is a file.
    #[error("{0} is not a directory")]
    NotDir(String),
    /// A directory where a file was expected.
    #[error("{0} is a directory")]
    IsDir(String),
    /// A directory still holds entries.
    #[error("directory {0} still holds entries")]
    NotEmpty(String),
    /// A name the volume cannot record.
    #[error("{0} cannot be recorded on the volume")]
    BadName(String),
    /// A volume identifier outside the rules.
    #[error("{0} is not a valid volume identifier")]
    BadVolumeId(String),
    /// The change would not fit.
    #[error("the volume is full: {need} bytes needed, capacity {capacity}")]
    VolumeFull {
        /// Bytes the image would need.
        need: u64,
        /// Bytes allowed.
        capacity: u64,
    },
    /// The volume is write protected.
    #[error("the volume is write protected")]
    WriteProtected,
    /// Not a writable virtual volume (a plain ISO 9660 image).
    #[error("the volume can only be read")]
    ReadOnly,
    /// `INZOPT CHECK(*YES)` found files.
    #[error("the volume still holds files")]
    ActiveFiles,
    /// A request that makes no sense here.
    #[error("{0}")]
    Invalid(String),
}

impl Error {
    /// The message identifier for this error (chosen by Quartz where
    /// IBM's is not documented; see docs/MEDIA.md).
    #[must_use]
    pub fn msgid(&self) -> &'static str {
        match self {
            Self::Io(_) | Self::Malformed(_) => "OPT1605",
            Self::NotFound(_) => "OPT1115",
            Self::Exists(_) => "OPT1210",
            Self::NotDir(_) | Self::IsDir(_) | Self::NotEmpty(_) | Self::Invalid(_) => "OPT1203",
            Self::BadName(_) => "OPT1317",
            Self::BadVolumeId(_) => "OPT1331",
            Self::VolumeFull { .. } => "OPT1335",
            Self::WriteProtected | Self::ReadOnly => "OPT1520",
            Self::ActiveFiles => "OPT1330",
        }
    }

    pub(crate) fn bad(what: impl Into<String>) -> Self {
        Self::Malformed(what.into())
    }
}
