// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The crate's errors and their message identifiers.

/// Result alias.
pub type Result<T> = std::result::Result<T, Error>;

/// What went wrong.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// No catalog of that name.
    #[error("image catalog {0} does not exist")]
    NotFound(String),
    /// The name is taken.
    #[error("image catalog {0} already exists")]
    Exists(String),
    /// Another catalog uses the directory.
    #[error("directory {dir} already belongs to image catalog {catalog}")]
    DirInUse {
        /// The directory.
        dir: String,
        /// The catalog using it.
        catalog: String,
    },
    /// The directory is missing.
    #[error("directory {0} does not exist")]
    NoDir(String),
    /// No entry at that index.
    #[error("image catalog {catalog} has no entry {index}")]
    NoEntry {
        /// The catalog.
        catalog: String,
        /// The index.
        index: u16,
    },
    /// The index is taken.
    #[error("index {0} is already in use")]
    IndexInUse(u16),
    /// No room for another entry.
    #[error("image catalog {0} is full")]
    Full(String),
    /// The image file is missing.
    #[error("image file {0} does not exist")]
    NoImage(String),
    /// The file is not an image of the catalog's type.
    #[error("{path} is not a usable image: {why}")]
    BadImage {
        /// The file.
        path: String,
        /// Why.
        why: String,
    },
    /// The catalog must be unloaded first.
    #[error("image catalog {0} is loaded")]
    Loaded(String),
    /// The device holds another catalog, or the catalog another device.
    #[error("{0}")]
    DeviceInUse(String),
    /// Two entries would carry one volume.
    #[error("volume {0} is already in the image catalog")]
    DuplicateVolume(String),
    /// Verification found problems.
    #[error("image catalog {catalog} did not verify: {problems} problems")]
    Verify {
        /// The catalog.
        catalog: String,
        /// How many.
        problems: usize,
    },
    /// The image cannot be written to.
    #[error("the image of entry {0} can only be read")]
    ReadOnlyImage(u16),
    /// A parameter value that is not allowed.
    #[error("{0}")]
    Invalid(String),
    /// The metadata file is damaged.
    #[error("the catalog file is damaged: {0}")]
    Damaged(String),
    /// From the optical layer.
    #[error(transparent)]
    Optical(#[from] optical::Error),
    /// From the file system.
    #[error("file system: {0}")]
    Io(#[from] std::io::Error),
}

impl Error {
    /// IBM's message identifier for the error's meaning (docs/MEDIA.md).
    /// A cause IBM gives no message of its own is `CPF9897`: the
    /// command sends it before its escape (`Image catalog not created`
    /// and the like).
    #[must_use]
    pub fn msgid(&self) -> &'static str {
        match self {
            Self::NotFound(_) => "CPFBC45",
            Self::Exists(_) => "CPFBC01",
            Self::NoEntry { .. } => "CPFBC4A",
            Self::Loaded(_) => "CPFBC4D",
            Self::DeviceInUse(_) => "CPFBC4B",
            Self::Verify { .. } => "CPFBC42",
            Self::Damaged(_) | Self::Io(_) => "CPFBC17",
            Self::DirInUse { .. }
            | Self::NoDir(_)
            | Self::IndexInUse(_)
            | Self::Full(_)
            | Self::NoImage(_)
            | Self::BadImage { .. }
            | Self::DuplicateVolume(_)
            | Self::ReadOnlyImage(_)
            | Self::Invalid(_) => "CPF9897",
            Self::Optical(e) => e.msgid(),
        }
    }
}
