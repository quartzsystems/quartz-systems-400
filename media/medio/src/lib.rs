// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The media under save and restore: a save stream (the bytes
//! `savfmt` writes) to and from a tape file, an optical volume's file,
//! or the volumes of an image catalog.
//!
//! - [`tapefile`]: one labeled tape file per save, in blocks of
//!   [`SAVE_BLOCK`], across volumes through a `VolumeProvider`.
//! - [`optfile`]: the save's file on an optical volume (`OPTFILE`).
//! - [`catalog`]: [`CatalogTapes`], the next volume of a virtual tape
//!   device from its image catalog's loaded entries; [`NextVolume`] the
//!   choice optical devices share.
//! - [`library`]: [`CatalogLibrary`], a virtual media library over a
//!   `*TAP` catalog, its cartridges, categories and moves.
//! - [`tapf`]: tape files (`*FILE TAPF`) and the records of `CPYTOTAP`
//!   and `CPYFRMTAP`.
//! - [`MediaWriter`] and [`MediaReader`]: what a save or restore
//!   command holds, whichever medium it names.
//!
//! See docs/MEDIA.md, "Save and restore on media".

#![deny(missing_docs)]

pub mod catalog;
pub mod error;
pub mod library;
pub mod optfile;
pub mod tapefile;
pub mod tapf;

pub use catalog::{CatalogTapes, NextVolume};
pub use library::{CatalogLibrary, LibraryState, LibraryVolumes};
pub use error::{MediaError, Result};

/// Bytes in each data block of a save on tape.
pub const SAVE_BLOCK: usize = 64 * 1024;

/// What a save ended with on its medium.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Written {
    /// The tape file's sequence number; 0 on other media.
    pub seq: u32,
    /// The volumes the save is on, in order.
    pub volumes: Vec<String>,
    /// The file label or optical path.
    pub file: String,
    /// Bytes written.
    pub bytes: u64,
}

/// Where a save stream goes.
pub trait MediaWriter {
    /// Write the whole stream as one file.
    fn write_stream(&mut self, data: &[u8]) -> Result<Written>;
}

/// Where a save stream comes from.
pub trait MediaReader {
    /// Read the whole stream back; the volumes it was on.
    fn read_stream(&mut self) -> Result<(Vec<u8>, Vec<String>)>;
}
