// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What a connection reaches of the system: the connection job's side.
//! The job implements it over its link to the storage service; tests
//! implement it in memory.

use std::sync::Arc;

use qap_proto::ops::command::CommandReply;
use qap_proto::ops::signon::SignOnReply;
use qap_proto::ops::system::SystemInfo;
use qap_proto::{Secret, ServerMessage, Status, Token};

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

/// A sign-on, password change or resume refused: the status and the
/// message a 5250 display would show for the same case.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refusal {
    /// `SignOnFailed`, `PasswordExpired`, `SessionExpired`, ...
    pub status: Status,
    /// The message.
    pub message: ServerMessage,
}

/// How a command ended: `Ok`, `Error` (its escape message is the
/// reply's message), `NotFound`, `NotAuthorized` or `Cancelled`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ran {
    /// The status.
    pub status: Status,
    /// The escape message, when one ended it.
    pub escape: Option<ServerMessage>,
    /// The job and the messages.
    pub reply: CommandReply,
}

/// Ends the operation running now, from another thread: what `Cancel`
/// of a running request calls.
pub type Canceller = Arc<dyn Fn() + Send + Sync>;

/// The connection job's reach.
pub trait Host {
    /// The system's name, release, time zone and CCSID.
    fn system_id(&mut self) -> SystemId;
    /// Milliseconds since 1970-01-01 UTC.
    fn now_ms(&mut self) -> i64;
    /// Message `id` with substitution data `data`, its text in the
    /// language of the job's user now: the system's before sign-on, the
    /// signed-on profile's after it. The connection asks while the job
    /// still runs as the user a message is for.
    fn message(&mut self, id: &str, data: &[&str]) -> ServerMessage;
    /// The system's description and installed products.
    fn system_info(&mut self) -> SystemInfo;
    /// Whether the connection is to end.
    fn ending(&mut self) -> Option<Ending>;

    /// Sign on as a 5250 display would; on success the job runs as the
    /// user and a session exists.
    fn sign_on(&mut self, user: &str, password: &Secret) -> Result<SignOnReply, Refusal>;
    /// Change `user`'s password; signed on as [`Host::sign_on`] when the
    /// connection was not. In a signed-on connection the reply's token
    /// is all zeros: the session goes on with its own.
    fn change_password(&mut self, user: &str, old: &Secret, new: &Secret) -> Result<SignOnReply, Refusal>;
    /// Attach this connection to the session `token` names.
    fn resume(&mut self, token: &Token) -> Result<SignOnReply, Refusal>;
    /// End the session; the job runs as its own user again.
    fn sign_off(&mut self, token: &Token);
    /// The connection closed: the session waits to be resumed.
    fn detach(&mut self, token: &Token);

    /// Run a CL command as the signed-on profile; every message it sent
    /// its caller, oldest first.
    fn run_command(&mut self, command: &str) -> Ran;
    /// Check a CL command without running it: `Ok` with no messages
    /// when valid, else `Error` (or `NotFound`) with the check's.
    fn check_command(&mut self, command: &str) -> (Status, Vec<ServerMessage>);
    /// What ends a running operation when the client cancels it.
    fn canceller(&mut self) -> Option<Canceller> {
        None
    }
}
