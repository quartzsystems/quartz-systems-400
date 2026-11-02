// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What a connection reaches of the system: the connection job's side.
//! The job implements it over its link to the storage service; tests
//! implement it in memory.

use qap_proto::ServerMessage;

/// Why the connection is to end, from outside it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ending {
    /// `ENDTCPSVR` or the subsystem: `ServerEnding`, then close.
    Server,
    /// An operator ended the job: `SessionEnded` when signed on, then
    /// close.
    Operator,
}

/// What `Hello` reports of the system.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SystemId {
    /// The system's name.
    pub system_name: String,
    /// Its release.
    pub release: String,
    /// The time zone description (`QTIMZON`).
    pub time_zone: String,
    /// `QCCSID`.
    pub ccsid: u16,
}

/// The connection job's reach.
pub trait Host {
    /// The system's name, release, time zone and CCSID.
    fn system_id(&mut self) -> SystemId;
    /// Milliseconds since 1970-01-01 UTC.
    fn now_ms(&mut self) -> i64;
    /// Message `id` with substitution data `data`, its text in the
    /// language of the job's user now.
    fn message(&mut self, id: &str, data: &[&str]) -> ServerMessage;
    /// Whether the connection is to end.
    fn ending(&mut self) -> Option<Ending>;
}
