// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Hardware resources: the one table of the machine's devices with their
//! IBM-style names (`CMBnn`, `LINnn`, `CMNnn`, `DCnn`, `DDnnn`, `OPTnn`,
//! `TAPnn`, `TAPMLBnn`), built from a sysfs walk and kept current by
//! hotplug events, with its packaging and logical trees.
//!
//! One process owns the table: the hardware resource service
//! (`/init qshwr`, `lic/slic/src/hwsvc/`). Everything else asks it over
//! [`proto`]; [`client`] connects to its socket.

#![forbid(unsafe_code)]

pub mod client;
pub mod proto;
pub mod scan;
pub mod table;
pub mod uevent;
pub mod view;

pub use scan::{nic, pci, storage};
pub use table::tree::TreeRow;
pub use table::{Kind, ParityCapable, Resource, Status, Table};
