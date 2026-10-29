// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The QS/400 integrated file system below the job and service
//! plumbing: IBM `errno` values ([`errno`]), paths and case folding
//! ([`path`]), the storage backend ([`backend`]), the one metadata layer
//! ([`meta`]), authority ([`auth`]), the mounted file systems ([`vfs`]),
//! the resolver ([`resolve`]), locks ([`lock`]), text conversion
//! ([`text`]), the operations ([`ops`]), the save walk ([`save`]) and
//! the interface a job calls them through ([`client`]). `docs/IFS.md`
//! describes the design.

#![deny(missing_docs)]
#![forbid(unsafe_code)]

pub mod auth;
pub mod backend;
pub mod client;
pub mod errno;
pub mod lock;
pub mod meta;
pub mod ops;
pub mod path;
pub mod resolve;
pub mod save;
pub mod text;
pub mod vfs;

pub use errno::{Errno, Result};
pub use ops::{Config, Ctx, Ifs, Owner};
