// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Errors.

use thiserror::Error;

/// What went wrong with install media.
#[derive(Debug, Error)]
pub enum Error {
    /// Not install media, or damaged.
    #[error("{0}")]
    Media(String),
    /// A response file line that does not parse.
    #[error("INSTALL.RSP line {line}: {what}")]
    Response {
        /// Line number from 1.
        line: usize,
        /// What is wrong.
        what: String,
    },
    /// The manifest's signature does not check out.
    #[error("the media manifest's signature is not valid: {0}")]
    Signature(String),
    /// The release rule refuses the install.
    #[error(transparent)]
    Upgrade(#[from] release::UpgradeError),
    /// The tape layer.
    #[error("tape: {0}")]
    Tape(#[from] tape::TapeError),
    /// The optical layer.
    #[error("optical: {0}")]
    Optical(#[from] optical::Error),
    /// The install of a product load.
    #[error("{0}")]
    Install(String),
}

impl Error {
    /// A media error.
    pub fn media(s: impl Into<String>) -> Self {
        Self::Media(s.into())
    }
}

/// A result with [`Error`].
pub type Result<T> = std::result::Result<T, Error>;
