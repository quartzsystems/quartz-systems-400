// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Errors of the disk layout crate.

use std::io;

/// Why a disk could not be laid out, written or found.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The target is smaller than the layout needs.
    #[error("disk of {bytes} bytes is smaller than the {min}-byte minimum")]
    TooSmall {
        /// Size offered.
        bytes: u64,
        /// Size needed.
        min: u64,
    },
    /// An I/O error on the target.
    #[error("{what}: {source}")]
    Io {
        /// What was being done.
        what: &'static str,
        /// The OS error.
        #[source]
        source: io::Error,
    },
    /// The GPT library refused.
    #[error("GPT {what}: {detail}")]
    Gpt {
        /// What was being done.
        what: &'static str,
        /// Its message.
        detail: String,
    },
    /// The FAT library refused, or the volume came out wrong.
    #[error("FAT {what}: {detail}")]
    Fat {
        /// What was being done.
        what: &'static str,
        /// Its message.
        detail: String,
    },
    /// No partition of the requested type.
    #[error("no partition of type {0}")]
    NotFound(uuid::Uuid),
}

impl Error {
    /// Wrap an I/O error.
    pub fn io(what: &'static str) -> impl FnOnce(io::Error) -> Self {
        move |source| Self::Io { what, source }
    }
}
