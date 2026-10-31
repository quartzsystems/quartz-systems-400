// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Optical media for QS/400: ISO 9660 images with Joliet and Rock Ridge
//! (writer and reader), the hybrid install image, writable virtual
//! volumes and the commands' operations on them, physical devices, and
//! the set of loaded volumes `/QOPT` shows. See docs/MEDIA.md.

#![deny(missing_docs)]

pub mod crc32;
pub mod error;
pub mod iso;
pub mod manifest;
pub mod source;

pub use error::{Error, Result};
pub use iso::{Entry, EntryKind, IsoBuilder, IsoImage, VolumeInfo};
pub use source::{FileImage, ImageStore, MemImage, ReadAt, Slice};
