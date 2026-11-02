// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The operations of each service, their codes and bodies. Bodies not
//! listed are empty (`()`).

pub mod command;
pub mod connection;
pub mod signon;
pub mod spool;
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
    /// `Spool` 1: spooled files, filtered (version 3).
    ListSpooledFiles,
    /// `Spool` 2: a spooled file's pages as SCS, text or PDF (version 3).
    ReadSpooledFile,
    /// `Spool` 3: hold a spooled file (version 3).
    HoldSpooledFile,
    /// `Spool` 4: release a spooled file (version 3).
    ReleaseSpooledFile,
    /// `Spool` 5: delete a spooled file (version 3).
    DeleteSpooledFile,
    /// `Spool` 6: move a spooled file to another queue (version 3).
    MoveSpooledFile,
    /// `Spool` 7: output queues (version 3).
    ListOutputQueues,
}

impl Op {
    /// Every operation.
    pub const ALL: [Op; 19] = [
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
        Op::ListSpooledFiles,
        Op::ReadSpooledFile,
        Op::HoldSpooledFile,
        Op::ReleaseSpooledFile,
        Op::DeleteSpooledFile,
        Op::MoveSpooledFile,
        Op::ListOutputQueues,
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
            Op::ListSpooledFiles => (Service::Spool, 1),
            Op::ReadSpooledFile => (Service::Spool, 2),
            Op::HoldSpooledFile => (Service::Spool, 3),
            Op::ReleaseSpooledFile => (Service::Spool, 4),
            Op::DeleteSpooledFile => (Service::Spool, 5),
            Op::MoveSpooledFile => (Service::Spool, 6),
            Op::ListOutputQueues => (Service::Spool, 7),
        }
    }

    /// The protocol version that defines it.
    #[must_use]
    pub const fn since(self) -> u16 {
        match self {
            Op::Hello | Op::Ping | Op::SignOn | Op::ChangePassword | Op::Resume | Op::SignOff | Op::SystemInfo => 1,
            Op::TelnetTicket | Op::PasswordStatus | Op::RunCommand | Op::CheckCommand | Op::DescribeCommand => 2,
            Op::ListSpooledFiles
            | Op::ReadSpooledFile
            | Op::HoldSpooledFile
            | Op::ReleaseSpooledFile
            | Op::DeleteSpooledFile
            | Op::MoveSpooledFile
            | Op::ListOutputQueues => 3,
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
