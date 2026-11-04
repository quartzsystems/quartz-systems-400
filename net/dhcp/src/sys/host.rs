// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Off Linux there is no link to bind to: opening fails.

use std::io;
use std::time::Duration;

use crate::client::Dest;

/// A socket that cannot be opened here.
#[derive(Debug)]
pub struct Socket;

impl Socket {
    /// Always unsupported off Linux.
    pub fn open(_link: &str) -> io::Result<Self> {
        Err(io::Error::new(io::ErrorKind::Unsupported, "DHCP needs Linux"))
    }

    /// Never reached.
    pub fn send(&self, _to: Dest, _bytes: &[u8]) -> io::Result<()> {
        Err(io::Error::new(io::ErrorKind::Unsupported, "DHCP needs Linux"))
    }

    /// Never reached.
    pub fn recv(&self, _wait: Duration) -> io::Result<Option<Vec<u8>>> {
        Err(io::Error::new(io::ErrorKind::Unsupported, "DHCP needs Linux"))
    }
}
