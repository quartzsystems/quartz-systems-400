// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The Screen Design Aid's logic: the display-file source member and
//! its stable round trip ([`source`]), the design image of a record and
//! what the operator types on it ([`image`]), the keyword groups
//! ([`keywords`]), menu design ([`menu`]) and SDA's displays
//! ([`screen`]). No store and no 5250 I/O: the job's driver
//! (`work/qsjob/src/devtools/sda.rs`) does those.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod image;
pub mod keywords;
pub mod menu;
pub mod screen;
pub mod source;

pub use dds::lex::Keyword;
pub use source::{Member, Record, parse, write};
