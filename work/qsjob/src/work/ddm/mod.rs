// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! DDM and DRDA in the job (`net/ddm` has the datastream): the
//! listener `QRWTLSTN`, the server job `QRWTSRVR` with its backends
//! over the storage service, and the requester behind DDM files.

pub mod backend;
pub mod listener;
pub mod requester;
pub mod server;

use stg::{Client, Shared};

/// The role of the listener job (`net/servers` registry).
pub const ROLE: &str = "DDM";

/// This system's name (`QSYSNAME`).
pub fn system_name(link: &Shared) -> String {
    link.borrow_mut()
        .with(|l| l.sysval("QSYSNAME".into()))
        .map_or_else(
            || "QS400".to_string(),
            |v| ccsid::from_ebcdic(&v.data).trim().to_string(),
        )
}
