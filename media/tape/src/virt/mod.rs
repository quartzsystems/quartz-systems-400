// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Virtual tape: a volume in one file, and a library of them.

pub mod format;
mod library;
mod tape;

pub use library::{DRIVE_BASE, SLOT_BASE, STATION_BASE, TRANSPORT_ADDRESS, VirtualLibrary};
pub use tape::{DEFAULT_DENSITY, VirtualTape, VolumeOptions};
