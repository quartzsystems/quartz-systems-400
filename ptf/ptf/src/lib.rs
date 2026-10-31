// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! PTFs (`docs/PTF.md`): the header and stream, the states `DSPPTF`
//! shows, the system's PTF database, load, temporary and permanent
//! apply and remove over the store, and PTF groups.

#![deny(missing_docs)]

pub mod apply;
pub mod db;
mod error;
pub mod group;
pub mod grpdb;
pub mod header;
pub mod install;
pub mod manifest;
pub mod package;
pub mod slot;
pub mod spec;
pub mod status;

pub use db::{PtfDb, PtfRecord};
pub use error::{Error, Result};
pub use header::{ExitProgram, ExitWhen, PtfHeader, PtfKind, PtfRef};
pub use status::{Action, Status};

/// Type code of the header object in a PTF stream (never stored).
pub const HEADER_TYPE: u16 = 0xFFF0;
/// Type code of a LIC image in a PTF stream (never stored).
pub const LIC_TYPE: u16 = 0xFFF1;
