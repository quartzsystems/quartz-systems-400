// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The QS/400 HTTP server's instances (docs/HTTPADMIN.md):
//!
//! - [`conf`]: `httpd.conf`, parsed, checked, edited and printed.
//! - [`files`]: the files instances live in, by IFS path.
//! - [`registry`]: `/www/instances.json`, every instance's entry.
//! - [`instance`]: creating, deleting and configuring an instance.

pub mod conf;
pub mod files;
pub mod instance;
pub mod registry;

/// Why an instance operation failed.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    /// The configuration does not parse (`HTP8100`).
    #[error("configuration error at {0}")]
    Conf(conf::ConfError),
    /// Not a valid instance name.
    #[error("{0} is not a valid instance name")]
    BadName(String),
    /// The instance already exists.
    #[error("instance {0} already exists")]
    Exists(String),
    /// No such instance.
    #[error("instance {0} not found")]
    NotFound(String),
    /// A file could not be read or written.
    #[error("{0}: {1}")]
    Io(String, String),
    /// The registry is damaged.
    #[error("registry damaged: {0}")]
    Damaged(String),
}

/// A result of this crate.
pub type Result<T> = std::result::Result<T, Error>;
