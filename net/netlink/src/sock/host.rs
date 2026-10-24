// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Host stub: there is no netlink off Linux.

use std::io;
use std::time::Duration;

use super::Bus;
use crate::Error;

/// Never constructed.
pub enum Raw {}

impl Raw {
    /// Always [`Error::Unsupported`].
    pub fn open(_bus: Bus, _groups: u32) -> Result<Self, Error> {
        Err(Error::Unsupported)
    }

    /// Unreachable.
    pub fn set_timeout(&self, _t: Option<Duration>) -> io::Result<()> {
        match *self {}
    }

    /// Unreachable.
    pub fn send(&self, _b: &[u8]) -> io::Result<()> {
        match *self {}
    }

    /// Unreachable.
    pub fn recv(&self, _buf: &mut [u8]) -> io::Result<usize> {
        match *self {}
    }
}
