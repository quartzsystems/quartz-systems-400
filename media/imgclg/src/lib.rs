// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Image catalogs (`*IMGCLG`) for QS/400: catalogs of virtual optical
//! images (`TYPE(*OPT)`) and virtual tape volumes (`TYPE(*TAP)`) in an
//! IFS directory, their entries and states, the metadata file kept in
//! the directory, and the commands. Runs over [`fs::CatalogFs`]. See
//! docs/MEDIA.md.

#![deny(missing_docs)]

pub mod error;
pub mod fs;
pub mod model;
pub mod ops;
pub mod tape;

pub use error::{Error, Result};
pub use fs::{CatalogFs, MemFs, StdFs};
pub use model::{Catalog, CatalogStatus, Entry, EntryStatus, ImgType};
pub use ops::Registry;
pub use tape::{NoTape, TapeImages};
