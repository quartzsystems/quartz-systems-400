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
    /// The message identifier (chosen by Quartz; see docs/MEDIA.md).
    #[must_use]
    pub fn msgid(&self) -> &'static str {
        match self {
            Self::NotFound(_) => "CPFBC01",
            Self::Exists(_) => "CPFBC02",
            Self::DirInUse { .. } => "CPFBC03",
            Self::NoDir(_) => "CPFBC04",
            Self::NoEntry { .. } => "CPFBC05",
            Self::IndexInUse(_) => "CPFBC06",
            Self::Full(_) => "CPFBC07",
            Self::NoImage(_) => "CPFBC08",
            Self::BadImage { .. } => "CPFBC09",
            Self::Loaded(_) => "CPFBC0A",
            Self::DeviceInUse(_) => "CPFBC0B",
            Self::DuplicateVolume(_) => "CPFBC0C",
            Self::Verify { .. } => "CPFBC0D",
            Self::ReadOnlyImage(_) => "CPFBC0E",
            Self::Invalid(_) => "CPFBC0F",
            Self::Damaged(_) | Self::Io(_) => "CPFBC10",
            Self::Optical(e) => e.msgid(),
        }
    }
}
