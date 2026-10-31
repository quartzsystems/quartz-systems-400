// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Install media (docs/INSTALL.md, docs/RECOVERY.md):
//!
//! - [`layout`]: what install media holds and where: the LIC, the
//!   operating system's product load, the licensed programs, and on a
//!   `SAVSYS` volume the security data and the configuration.
//! - [`sign`]: the manifest's signature with the build key.
//! - [`rsp`]: the response file of an automatic install.
//! - [`savsys`]: the layout on a tape and on an optical volume, written
//!   and recognised.
//! - [`osload`]: the operating system install over a store: the
//!   `QS00SS1` load restored through the RSTLICPGM planning, as a fresh
//!   install or as a product replacement over an older one.

#![deny(missing_docs)]

pub mod error;
pub mod layout;
pub mod osload;
pub mod rsp;
pub mod savsys;
pub mod sign;

pub use error::{Error, Result};
pub use layout::{Contents, Payload};
pub use rsp::Response;
