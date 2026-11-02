// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The operations of each service, their codes and bodies. Bodies not
//! listed are empty (`()`).

pub mod command;
pub mod connection;
pub mod signon;
pub mod system;

use crate::envelope::Service;

/// An operation a client may request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    /// `Connection` 1: agree on a version; the first request.
    Hello,
    /// `Connection` 2: keep the connection alive.
    Ping,
    /// `SignOn` 1: sign on with a password.
    SignOn,
    /// `SignOn` 2: change a password, signing on.
    ChangePassword,
    /// `SignOn` 3: re-attach a session by its token.
    Resume,
    /// `SignOn` 4: end the session.
    SignOff,
    /// `System` 1: the system's description.
    SystemInfo,
    /// `SignOn` 5: a one-time ticket for 5250 automatic sign-on
    /// (version 2).
    TelnetTicket,
    /// `SignOn` 6: when the password expires (version 2).
    PasswordStatus,
    /// `Command` 1: run a CL command (version 2).
    RunCommand,
    /// `Command` 2: check a CL command without running it (version 2).
    CheckCommand,
    /// `Command` 3: a command's definition, for prompting (version 2).
    DescribeCommand,
}

impl Op {
    /// Every operation.
    pub const ALL: [Op; 12] = [
        Op::Hello,
        Op::Ping,
        Op::SignOn,
        Op::ChangePassword,
        Op::Resume,
        Op::SignOff,
        Op::SystemInfo,
        Op::TelnetTicket,
        Op::PasswordStatus,
        Op::RunCommand,
        Op::CheckCommand,
        Op::DescribeCommand,
    ];

    /// Its service and code.
    #[must_use]
    pub const fn code(self) -> (Service, u16) {
        match self {
            Op::Hello => (Service::Connection, 1),
            Op::Ping => (Service::Connection, 2),
            Op::SignOn => (Service::SignOn, 1),
            Op::ChangePassword => (Service::SignOn, 2),
            Op::Resume => (Service::SignOn, 3),
            Op::SignOff => (Service::SignOn, 4),
            Op::SystemInfo => (Service::System, 1),
            Op::TelnetTicket => (Service::SignOn, 5),
            Op::PasswordStatus => (Service::SignOn, 6),
            Op::RunCommand => (Service::Command, 1),
            Op::CheckCommand => (Service::Command, 2),
            Op::DescribeCommand => (Service::Command, 3),
        }
    }

    /// The protocol version that defines it.
    #[must_use]
    pub const fn since(self) -> u16 {
        match self {
            Op::Hello | Op::Ping | Op::SignOn | Op::ChangePassword | Op::Resume | Op::SignOff | Op::SystemInfo => 1,
            Op::TelnetTicket | Op::PasswordStatus | Op::RunCommand | Op::CheckCommand | Op::DescribeCommand => 2,
        }
    }

    /// The operation a service and code name.
    #[must_use]
    pub fn of(service: Service, code: u16) -> Option<Op> {
        Op::ALL.into_iter().find(|o| o.code() == (service, code))
    }

    /// Whether it is allowed before sign-on.
    #[must_use]
    pub const fn before_signon(self) -> bool {
        matches!(self, Op::Hello | Op::Ping | Op::SignOn | Op::ChangePassword | Op::Resume)
    }
}

/// Event codes, server to client.
pub mod event {
    /// `Connection` 100: the server is ending and closes the connection.
    pub const SERVER_ENDING: u16 = 100;
    /// `SignOn` 100: the session ended.
    pub const SESSION_ENDED: u16 = 100;
}
