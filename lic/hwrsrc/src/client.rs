// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Reaching the hardware resource service: a connection per question to
//! its socket, so any process may ask without keeping one open.

use std::path::Path;

use ipc::WireError;

use crate::proto;
use crate::table::{Resource, Table};

/// The service's socket.
pub const SOCKET: &str = "/run/qs400/hwrsrc.sock";

/// One connection to the service.
#[cfg(unix)]
pub struct Conn(ipc::chan::Chan);

#[cfg(unix)]
impl Conn {
    /// Connect to the socket at `path`.
    pub fn connect_to(path: &Path) -> Result<Self, WireError> {
        let c = ipc::chan::Chan::connect(path)?;
        c.set_timeout(Some(std::time::Duration::from_secs(30)))?;
        Ok(Self(c))
    }

    /// Connect to the service.
    pub fn connect() -> Result<Self, WireError> {
        Self::connect_to(Path::new(SOCKET))
    }
}

#[cfg(unix)]
impl proto::Client for Conn {
    fn call(&mut self, req: &proto::Request) -> Result<Vec<u8>, WireError> {
        self.0.send(req)?;
        self.0.recv_raw()
    }
}

/// The table, or why not.
#[cfg(unix)]
pub fn table(refresh: bool) -> Result<Table, WireError> {
    use proto::Client as _;
    Conn::connect()?.table(refresh)
}

/// One resource, or why not.
#[cfg(unix)]
pub fn resource(name: &str) -> Result<Option<Resource>, WireError> {
    use proto::Client as _;
    Conn::connect()?.resource(name.to_string())
}

/// Walk sysfs again now.
#[cfg(unix)]
pub fn rescan() -> Result<Vec<String>, WireError> {
    use proto::Client as _;
    Conn::connect()?.rescan()
}
