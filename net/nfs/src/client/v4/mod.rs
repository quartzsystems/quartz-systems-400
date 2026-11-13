// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! NFS versions 4.0 and 4.1 mounts.

use std::sync::Arc;

use ifs::Errno;
use ifs::remote::{RResult, RemoteError};

use super::Target;
use super::channel::Status;
use super::settings::{Settled, Version};

/// A version 4 mount.
#[derive(Debug)]
pub struct V4 {
    version: Version,
}

impl V4 {
    /// The minor version in use.
    #[must_use]
    pub fn version(&self) -> Version {
        self.version
    }

    /// Locks held.
    #[must_use]
    pub fn locks_held(&self) -> usize {
        0
    }
}

/// Mount `t` with version 4.1 (`minor1`) or 4.0.
pub(super) fn mount(
    _t: &Target<'_>,
    _status: &Arc<Status>,
    _minor1: bool,
) -> RResult<(V4, Settled)> {
    Err(RemoteError::Errno(Errno::EPROTONOSUPPORT))
}

mod fs;
