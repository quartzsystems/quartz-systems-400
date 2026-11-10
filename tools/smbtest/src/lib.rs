// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The `NetServer` test client's scenarios (`docs/NETSERVER.md` section 11).
//!
//! The client is the published `smb` crate (afiffon/smb-rs), written by other
//! authors than the server library QS/400 vendors, so a misreading of
//! [MS-SMB2] cannot hide in both. Nothing here depends on `net/netserver`.

#![deny(missing_docs)]

pub mod conn;
pub mod error;
pub mod report;
pub mod scenarios;
pub mod target;

pub use error::Error;
pub use report::{Outcome, Report};
pub use target::Target;
