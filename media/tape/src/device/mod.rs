// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The device and library data model: resource names, cartridges,
//! categories and drive allocation. No commands live here.

pub mod cartridge;
pub mod naming;

pub use cartridge::{
    Allocation, Cartridge, Category, DriveResource, LibraryError, Location, MediaLibrary, Move,
    Selection, cartridge_id, execute,
};
pub use naming::{Resource, ResourceKind, Scanned, assign, parse_hctl, tape_nodes};
