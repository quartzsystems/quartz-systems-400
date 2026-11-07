// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The envelope's enums: what a frame is, which service it is for, and
//! how a reply or event ended. Values are appended, never renumbered.

use ipc::wire_enum;

/// What a frame is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// Client to server.
    Request,
    /// Server to client, answering a request.
    Reply,
    /// Server to client, unasked; `request_id` 0.
    Event,
    /// Client to server: stop the request `request_id` names.
    Cancel,
}

wire_enum!(Kind {
    0 => Request,
    1 => Reply,
    2 => Event,
    3 => Cancel,
});

/// The service a frame is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Service {
    /// Hello, Ping and the connection's events.
    Connection,
    /// Sign-on, sessions and passwords.
    SignOn,
    /// Information about the system.
    System,
    /// CL commands (version 2).
    Command,
    /// Spooled files and output queues (version 3).
    Spool,
    /// SQL and the catalog (version 4).
    Sql,
    /// Integrated file system files (version 5).
    Ifs,
    /// Message queues (version 6).
    Messages,
    /// Managing the system (version 7, draft).
    Manage,
    /// The source debugger (version 9, draft).
    Debug,
}

wire_enum!(Service {
    0 => Connection,
    1 => SignOn,
    2 => System,
    3 => Command,
    4 => Spool,
    5 => Sql,
    6 => Ifs,
    7 => Messages,
    8 => Manage,
    11 => Debug,
});

impl Service {
    /// Every service, in value order.
    pub const ALL: [Service; 10] = [
        Service::Connection,
        Service::SignOn,
        Service::System,
        Service::Command,
        Service::Spool,
        Service::Sql,
        Service::Ifs,
        Service::Messages,
        Service::Manage,
        Service::Debug,
    ];

    /// The protocol version that defines it.
    #[must_use]
    pub const fn since(self) -> u16 {
        match self {
            Service::Connection | Service::SignOn | Service::System => 1,
            Service::Command => 2,
            Service::Spool => 3,
            Service::Sql => 4,
            Service::Ifs => 5,
            Service::Messages => 6,
            Service::Manage => 7,
            Service::Debug => 9,
        }
    }

    /// The services version `v` defines, in value order.
    #[must_use]
    pub fn of_version(v: u16) -> Vec<Service> {
        Service::ALL
            .into_iter()
            .filter(|s| s.since() <= v)
            .collect()
    }
}

/// How a reply or event ended; `Ok` on requests. A `u16` on the wire.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// Done.
    Ok,
    /// Failed; the message says what.
    Error,
    /// No version in the client's range.
    VersionUnsupported,
    /// The operation needs a signed-on session.
    NotSignedOn,
    /// Sign-on refused.
    SignOnFailed,
    /// The password expired; change it.
    PasswordExpired,
    /// The profile may not do this.
    NotAuthorized,
    /// What the request names does not exist.
    NotFound,
    /// The request is not valid here.
    InvalidRequest,
    /// Too many requests outstanding.
    Busy,
    /// Cancelled by the client.
    Cancelled,
    /// The session is gone.
    SessionExpired,
    /// The server is ending.
    ServerEnding,
    /// The name is taken (version 5).
    Exists,
    /// Another job holds the object (version 3).
    InUse,
    /// An SQL statement failed; the body is `SqlFailure` (version 4).
    SqlError,
}

wire_enum!(Status: u16 {
    0 => Ok,
    1 => Error,
    2 => VersionUnsupported,
    3 => NotSignedOn,
    4 => SignOnFailed,
    5 => PasswordExpired,
    6 => NotAuthorized,
    7 => NotFound,
    8 => InvalidRequest,
    9 => Busy,
    10 => Cancelled,
    11 => SessionExpired,
    12 => ServerEnding,
    13 => Exists,
    14 => InUse,
    15 => SqlError,
});

impl Status {
    /// The protocol version that defines it. Version 2 defines no new
    /// status; version 3 defines `InUse`, version 4 `SqlError`, version 5
    /// `Exists`.
    #[must_use]
    pub const fn since(self) -> u16 {
        match self {
            Status::InUse => 3,
            Status::SqlError => 4,
            Status::Exists => 5,
            _ => 1,
        }
    }

    /// The status a version-`v` peer is sent: itself, or `Error` (with
    /// the same message) when `v` does not define it.
    #[must_use]
    pub const fn for_version(self, v: u16) -> Status {
        if self.since() <= v {
            self
        } else {
            Status::Error
        }
    }

    /// Every status, in value order.
    pub const ALL: [Status; 16] = [
        Status::Ok,
        Status::Error,
        Status::VersionUnsupported,
        Status::NotSignedOn,
        Status::SignOnFailed,
        Status::PasswordExpired,
        Status::NotAuthorized,
        Status::NotFound,
        Status::InvalidRequest,
        Status::Busy,
        Status::Cancelled,
        Status::SessionExpired,
        Status::ServerEnding,
        Status::Exists,
        Status::InUse,
        Status::SqlError,
    ];
}
