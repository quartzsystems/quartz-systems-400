// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! System Service Tools: what `STRSST` shows in a job. The screens and
//! their logic are DST's (`svctools`): the same panels, the same
//! functions behind them, reached from SST's own sign-on title and main
//! menu ([`menu`]) and asking for confirmation before a change to disks
//! or user IDs (the environment says so). [`screen`] turns a DST screen
//! into a `uim::Screen` for a job's display. No store and no 5250 I/O:
//! the job's driver (`work/qsjob/src/devtools/sst.rs`) asks the storage
//! service for the system's side.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod capacity;
pub mod menu;
pub mod screen;

pub use menu::{SstMenu, start};
pub use screen::{field_name, to_uim};
/// A DST screen as the console kit builds it.
pub use console::Screen as Grid;
