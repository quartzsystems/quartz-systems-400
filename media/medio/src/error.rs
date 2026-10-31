// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The errors of the media layer and their message identifiers.

use tape::TapeError;

/// Result alias.
pub type Result<T> = std::result::Result<T, MediaError>;

/// A medium refused or failed.
#[derive(Debug, thiserror::Error)]
pub enum MediaError {
    /// From the tape layer.
    #[error(transparent)]
    Tape(#[from] TapeError),
    /// From an optical volume.
    #[error(transparent)]
    Optical(#[from] optical::Error),
    /// The save is not on the medium.
    #[error("{0} was not found on the volume")]
    NotFound(String),
    /// The file is there and the save may not replace it.
    #[error("{0} already exists on the volume")]
    Exists(String),
}

impl MediaError {
    /// The message identifier the commands send.
    #[must_use]
    pub fn msgid(&self) -> &'static str {
        match self {
            Self::Tape(t) => t.message_id(),
            Self::Optical(o) => o.msgid(),
            Self::NotFound(_) => "OPT1115",
            Self::Exists(_) => "OPT1210",
        }
    }
}
