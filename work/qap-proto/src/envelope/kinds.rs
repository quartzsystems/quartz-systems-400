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
}

wire_enum!(Service {
    0 => Connection,
    1 => SignOn,
    2 => System,
});

impl Service {
    /// Every service, in value order.
    pub const ALL: [Service; 3] = [Service::Connection, Service::SignOn, Service::System];
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
});

impl Status {
    /// Every status, in value order.
    pub const ALL: [Status; 13] = [
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
    ];
}
