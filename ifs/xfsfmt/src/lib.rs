// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A pure-Rust mkfs for the IFS root: writes an empty, clean XFS v5
//! filesystem (CRCs, ftype, finobt, inobtcount, bigtime) onto any
//! seekable device. Output is a function of the size and [`Options`]
//! alone, so built images are reproducible. The on-disk choices are
//! documented in docs/IFS.md, "Disk format".

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod crc;
mod error;
mod format;
mod geom;
mod layout;
mod probe;
mod sparse;

pub use error::Error;
pub use format::{Options, Timestamp, format};
pub use geom::{BLOCK_SIZE, Geometry, INODE_SIZE, MAX_BYTES, MIN_BYTES};
pub use probe::{Info, probe};
pub use sparse::SparseDev;

/// The label QS/400 gives its IFS filesystem.
pub const LABEL: &str = "QS400-IFS";
