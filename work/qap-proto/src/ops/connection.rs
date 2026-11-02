// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `Connection`: `Hello` and `Ping`.

use ipc::{wire_enum, wire_struct};

use crate::envelope::Service;
use crate::types::JobId;

/// How a client may sign on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignOnMethod {
    /// User profile and password.
    Password,
}

wire_enum!(SignOnMethod {
    0 => Password,
});

/// `Hello`'s request.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Hello {
    /// The oldest version the client speaks.
    pub min_version: u16,
    /// The newest.
    pub max_version: u16,
    /// The client's name.
    pub client_name: String,
    /// Its version.
    pub client_version: String,
    /// Optional features it has.
    pub capabilities: Vec<String>,
}

wire_struct!(Hello {
    min_version,
    max_version,
    client_name,
    client_version,
    capabilities
});

/// `Hello`'s reply.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HelloReply {
    /// The version chosen.
    pub version: u16,
    /// The system's name.
    pub system_name: String,
    /// Its release (`V1R0M0`).
    pub release: String,
    /// The services offered.
    pub services: Vec<Service>,
    /// Optional features the server has.
    pub capabilities: Vec<String>,
    /// How the client may sign on.
    pub signon_methods: Vec<SignOnMethod>,
    /// Milliseconds since 1970-01-01 UTC.
    pub server_time: i64,
    /// The system's time zone description.
    pub time_zone: String,
    /// The system's CCSID.
    pub system_ccsid: u16,
}

wire_struct!(HelloReply {
    version,
    system_name,
    release,
    services,
    capabilities,
    signon_methods,
    server_time,
    time_zone,
    system_ccsid
});

/// `Ping`'s reply.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PingReply {
    /// Milliseconds since 1970-01-01 UTC.
    pub server_time: i64,
}

wire_struct!(PingReply { server_time });

/// `JobInfo`'s reply (version 4): the connection's job.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct JobInfoReply {
    /// The connection job, as the system names it.
    pub job: JobId,
    /// The profile it runs as now: the signed-on one, else the job's user.
    pub user: String,
}

wire_struct!(JobInfoReply { job, user });

/// The version to speak: the newest the server has in the client's
/// range.
#[must_use]
pub fn choose_version(min: u16, max: u16, server_max: u16) -> Option<u16> {
    let v = max.min(server_max);
    (min <= v && v >= 1).then_some(v)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions() {
        assert_eq!(choose_version(1, 1, 1), Some(1));
        assert_eq!(choose_version(1, 5, 1), Some(1));
        assert_eq!(choose_version(2, 5, 1), None);
        assert_eq!(choose_version(0, 0, 1), None);
        assert_eq!(choose_version(3, 2, 1), None);
    }
}
