// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! ISO 9660 (ECMA-119): the writer, the reader and their parts.

pub mod boot;
pub mod date;
pub mod hybrid;
pub mod name;
mod plan;
pub mod read;
pub mod susp;
pub mod tree;
pub mod write;

pub use date::Timestamp;
pub use read::{Entry, EntryKind, IsoImage, VolumeInfo};
pub use tree::{Content, Dir, Node};
pub use write::{FileExtent, IsoBuilder, Layout};

/// Logical sector size.
pub const SECTOR: usize = 2048;
/// The primary volume descriptor's sector.
pub const PVD_SECTOR: u32 = 16;
/// [`SECTOR`] as a `u32`.
pub const SECTOR_U32: u32 = 2048;
/// [`SECTOR`] as a `u16`.
pub const SECTOR_U16: u16 = 2048;
/// Default largest extent: the largest whole-sector length a 32-bit
/// field holds.
pub const EXTENT_LIMIT: u32 = 0xFFFF_F800;

/// Strings Quartz puts in the volume descriptors.
pub const PUBLISHER: &str = "QUARTZ SYSTEMS";
/// Data preparer.
pub const PREPARER: &str = "QS/400 BUILD";
/// Application identifier.
pub const APPLICATION: &str = "QS/400 INSTALLATION MEDIA";
/// System identifier.
pub const SYSTEM_ID: &str = "QS/400";

/// How an image is written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Options {
    /// The real volume identifier.
    pub volume_id: String,
    /// System identifier.
    pub system_id: String,
    /// Volume set identifier (empty: the volume id).
    pub volume_set: String,
    /// Volumes in the set.
    pub set_size: u16,
    /// This volume's number in the set.
    pub sequence: u16,
    /// Publisher.
    pub publisher: String,
    /// Data preparer.
    pub preparer: String,
    /// Application.
    pub application: String,
    /// Unix time of every timestamp.
    pub epoch: u64,
    /// Write a Joliet tree.
    pub joliet: bool,
    /// Write Rock Ridge names in the primary tree.
    pub rock_ridge: bool,
    /// The El Torito boot image's path.
    pub boot: Option<Vec<String>>,
    /// Prepend the hybrid MBR and GPT.
    pub hybrid: bool,
    /// Largest extent in bytes (whole sectors).
    pub extent_limit: u32,
}

impl Options {
    /// Defaults for a volume.
    #[must_use]
    pub fn new(volume_id: &str, epoch: u64) -> Self {
        Self {
            volume_id: volume_id.to_string(),
            system_id: SYSTEM_ID.to_string(),
            volume_set: String::new(),
            set_size: 1,
            sequence: 1,
            publisher: PUBLISHER.to_string(),
            preparer: PREPARER.to_string(),
            application: APPLICATION.to_string(),
            epoch,
            joliet: true,
            rock_ridge: true,
            boot: None,
            hybrid: false,
            extent_limit: EXTENT_LIMIT,
        }
    }

    pub(crate) fn date(&self) -> Timestamp {
        Timestamp::from_unix(self.epoch)
    }

    /// What the hybrid GUIDs are derived from.
    pub(crate) fn seed(&self) -> Vec<u8> {
        let mut s = self.volume_id.as_bytes().to_vec();
        s.extend_from_slice(&self.epoch.to_le_bytes());
        s
    }
}
