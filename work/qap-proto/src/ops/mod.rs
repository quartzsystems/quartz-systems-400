// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The operations of each service, their codes and bodies. Bodies not
//! listed are empty (`()`).

pub mod command;
pub mod connection;
pub mod develop;
pub mod ifs;
pub mod jobs;
pub mod manage;
pub mod messages;
pub mod signon;
pub mod spool;
pub mod sql;
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
    /// `SignOn` 7: add this connection to a session (version 4).
    Attach,
    /// `Sql` 1: commitment control, naming, default schema (version 4).
    SetOptions,
    /// `Sql` 2: prepare a statement (version 4).
    Prepare,
    /// `Sql` 3: run a prepared statement (version 4).
    Execute,
    /// `Sql` 4: rows from a cursor (version 4).
    Fetch,
    /// `Sql` 5: close a cursor (version 4).
    CloseCursor,
    /// `Sql` 6: release a prepared statement (version 4).
    Release,
    /// `Sql` 7: commit (version 4).
    Commit,
    /// `Sql` 8: roll back (version 4).
    Rollback,
    /// `Sql` 9: schemas in the catalog (version 4).
    ListSchemas,
    /// `Sql` 10: tables, views, aliases and files (version 4).
    ListTables,
    /// `Sql` 11: a table's columns (version 4).
    ListColumns,
    /// `Connection` 3: the connection's job (version 4).
    JobInfo,
    /// `Ifs` 1: a directory's entries (version 5).
    ListDirectory,
    /// `Ifs` 2: one entry (version 5).
    GetInfo,
    /// `Ifs` 3: a file's data, converted or not (version 5).
    ReadFile,
    /// `Ifs` 4: write a file (version 5).
    WriteFile,
    /// `Ifs` 5: make a directory (version 5).
    CreateDirectory,
    /// `Ifs` 6: delete a file, a link or an empty directory (version 5).
    Delete,
    /// `Ifs` 7: rename or move (version 5).
    Rename,
    /// `Ifs` 8: copy a file on the system (version 5).
    Copy,
    /// `Messages` 1: a queue's messages (version 6).
    ListMessages,
    /// `Messages` 2: answer an inquiry (version 6).
    ReplyMessage,
    /// `Messages` 3: remove messages (version 6).
    RemoveMessages,
    /// `Messages` 4: send a message (version 6).
    SendMessage,
    /// `Messages` 5: report the messages that arrive on a queue (version 6).
    WatchQueue,
    /// `Messages` 6: stop watching a queue (version 6).
    UnwatchQueue,
    /// `Spool` 8: report changes to a queue's or a user's files (version 6).
    WatchSpool,
    /// `Spool` 9: stop watching them (version 6).
    UnwatchSpool,
    /// `Ifs` 9: one entry and its change counter (version 6).
    Stat,
    /// `Manage` 1: subsystems and their states (version 7).
    ListSubsystems,
    /// `Manage` 2: start a subsystem (version 7).
    StartSubsystem,
    /// `Manage` 3: end a subsystem, or all (version 7).
    EndSubsystem,
    /// `Manage` 4: system values (version 7).
    GetSystemValues,
    /// `Manage` 5: change a system value (version 7).
    SetSystemValue,
    /// `Manage` 6: PTFs (version 7).
    ListPtfs,
    /// `Manage` 7: disk units and ASPs (version 7).
    ListDiskUnits,
    /// `Manage` 8: hardware resources of a type (version 7).
    ListHardwareResources,
    /// `Manage` 9: the IPL's state and the last SRC (version 7).
    IplStatus,
    /// `Manage` 10: power down, or restart (version 7).
    PowerDown,
    /// `Develop` 1: the connection job's library list (version 8).
    GetLibraryList,
    /// `Develop` 2: change it, as `CHGLIBL` (version 8).
    SetLibraryList,
    /// `Develop` 3: libraries (version 8).
    ListLibraries,
    /// `Develop` 4: objects (version 8).
    ListObjects,
    /// `Develop` 5: an object's description, as `DSPOBJD` (version 8).
    DescribeObject,
    /// `Develop` 6: a file's members (version 8).
    ListMembers,
    /// `Develop` 7: a source member's records (version 8).
    ReadMember,
    /// `Develop` 8: replace a source member's records (version 8).
    WriteMember,
    /// `Develop` 9: end the connection's lock on a member (version 8).
    UnlockMember,
    /// `Develop` 10: add a member, as `ADDPFM` (version 8).
    CreateMember,
    /// `Develop` 11: copy a member, as `CPYSRCF` (version 8).
    CopyMember,
    /// `Develop` 12: rename a member, as `RNMM` (version 8).
    RenameMember,
    /// `Develop` 13: remove a member, as `RMVM` (version 8).
    DeleteMember,
    /// `Develop` 14: a member's type and text, as `CHGPFM` (version 8).
    ChangeMember,
    /// `Develop` 15: run a compile and take its diagnostics (version 8).
    Compile,
    /// `Jobs` 1: a user's jobs, as `WRKUSRJOB` (version 8).
    ListJobs,
    /// `Jobs` 2: a job's log, as `DSPJOBLOG` (version 8).
    JobLog,
    /// `Jobs` 3: hold a job (version 8).
    HoldJob,
    /// `Jobs` 4: release a job (version 8).
    ReleaseJob,
    /// `Jobs` 5: end a job (version 8).
    EndJob,
    /// `Sql` 12: procedures and functions (version 8).
    ListRoutines,
    /// `Sql` 13: a table's indexes (version 8).
    ListIndexes,
    // Version 9 appends `Debug` (service 11) here.
}

impl Op {
    /// Every operation.
    pub const ALL: [Op; 81] = [
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
        Op::Attach,
        Op::SetOptions,
        Op::Prepare,
        Op::Execute,
        Op::Fetch,
        Op::CloseCursor,
        Op::Release,
        Op::Commit,
        Op::Rollback,
        Op::ListSchemas,
        Op::ListTables,
        Op::ListColumns,
        Op::JobInfo,
        Op::ListDirectory,
        Op::GetInfo,
        Op::ReadFile,
        Op::WriteFile,
        Op::CreateDirectory,
        Op::Delete,
        Op::Rename,
        Op::Copy,
        Op::ListMessages,
        Op::ReplyMessage,
        Op::RemoveMessages,
        Op::SendMessage,
        Op::WatchQueue,
        Op::UnwatchQueue,
        Op::WatchSpool,
        Op::UnwatchSpool,
        Op::Stat,
        Op::ListSubsystems,
        Op::StartSubsystem,
        Op::EndSubsystem,
        Op::GetSystemValues,
        Op::SetSystemValue,
        Op::ListPtfs,
        Op::ListDiskUnits,
        Op::ListHardwareResources,
        Op::IplStatus,
        Op::PowerDown,
        Op::GetLibraryList,
        Op::SetLibraryList,
        Op::ListLibraries,
        Op::ListObjects,
        Op::DescribeObject,
        Op::ListMembers,
        Op::ReadMember,
        Op::WriteMember,
        Op::UnlockMember,
        Op::CreateMember,
        Op::CopyMember,
        Op::RenameMember,
        Op::DeleteMember,
        Op::ChangeMember,
        Op::Compile,
        Op::ListJobs,
        Op::JobLog,
        Op::HoldJob,
        Op::ReleaseJob,
        Op::EndJob,
        Op::ListRoutines,
        Op::ListIndexes,
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
            Op::Attach => (Service::SignOn, 7),
            Op::SetOptions => (Service::Sql, 1),
            Op::Prepare => (Service::Sql, 2),
            Op::Execute => (Service::Sql, 3),
            Op::Fetch => (Service::Sql, 4),
            Op::CloseCursor => (Service::Sql, 5),
            Op::Release => (Service::Sql, 6),
            Op::Commit => (Service::Sql, 7),
            Op::Rollback => (Service::Sql, 8),
            Op::ListSchemas => (Service::Sql, 9),
            Op::ListTables => (Service::Sql, 10),
            Op::ListColumns => (Service::Sql, 11),
            Op::JobInfo => (Service::Connection, 3),
            Op::ListDirectory => (Service::Ifs, 1),
            Op::GetInfo => (Service::Ifs, 2),
            Op::ReadFile => (Service::Ifs, 3),
            Op::WriteFile => (Service::Ifs, 4),
            Op::CreateDirectory => (Service::Ifs, 5),
            Op::Delete => (Service::Ifs, 6),
            Op::Rename => (Service::Ifs, 7),
            Op::Copy => (Service::Ifs, 8),
            Op::ListMessages => (Service::Messages, 1),
            Op::ReplyMessage => (Service::Messages, 2),
            Op::RemoveMessages => (Service::Messages, 3),
            Op::SendMessage => (Service::Messages, 4),
            Op::WatchQueue => (Service::Messages, 5),
            Op::UnwatchQueue => (Service::Messages, 6),
            Op::WatchSpool => (Service::Spool, 8),
            Op::UnwatchSpool => (Service::Spool, 9),
            Op::Stat => (Service::Ifs, 9),
            Op::ListSubsystems => (Service::Manage, 1),
            Op::StartSubsystem => (Service::Manage, 2),
            Op::EndSubsystem => (Service::Manage, 3),
            Op::GetSystemValues => (Service::Manage, 4),
            Op::SetSystemValue => (Service::Manage, 5),
            Op::ListPtfs => (Service::Manage, 6),
            Op::ListDiskUnits => (Service::Manage, 7),
            Op::ListHardwareResources => (Service::Manage, 8),
            Op::IplStatus => (Service::Manage, 9),
            Op::PowerDown => (Service::Manage, 10),
            Op::GetLibraryList => (Service::Develop, 1),
            Op::SetLibraryList => (Service::Develop, 2),
            Op::ListLibraries => (Service::Develop, 3),
            Op::ListObjects => (Service::Develop, 4),
            Op::DescribeObject => (Service::Develop, 5),
            Op::ListMembers => (Service::Develop, 6),
            Op::ReadMember => (Service::Develop, 7),
            Op::WriteMember => (Service::Develop, 8),
            Op::UnlockMember => (Service::Develop, 9),
            Op::CreateMember => (Service::Develop, 10),
            Op::CopyMember => (Service::Develop, 11),
            Op::RenameMember => (Service::Develop, 12),
            Op::DeleteMember => (Service::Develop, 13),
            Op::ChangeMember => (Service::Develop, 14),
            Op::Compile => (Service::Develop, 15),
            Op::ListJobs => (Service::Jobs, 1),
            Op::JobLog => (Service::Jobs, 2),
            Op::HoldJob => (Service::Jobs, 3),
            Op::ReleaseJob => (Service::Jobs, 4),
            Op::EndJob => (Service::Jobs, 5),
            Op::ListRoutines => (Service::Sql, 12),
            Op::ListIndexes => (Service::Sql, 13),
        }
    }

    /// The protocol version that defines it.
    #[must_use]
    pub const fn since(self) -> u16 {
        match self {
            Op::Hello
            | Op::Ping
            | Op::SignOn
            | Op::ChangePassword
            | Op::Resume
            | Op::SignOff
            | Op::SystemInfo => 1,
            Op::TelnetTicket
            | Op::PasswordStatus
            | Op::RunCommand
            | Op::CheckCommand
            | Op::DescribeCommand => 2,
            Op::ListSpooledFiles
            | Op::ReadSpooledFile
            | Op::HoldSpooledFile
            | Op::ReleaseSpooledFile
            | Op::DeleteSpooledFile
            | Op::MoveSpooledFile
            | Op::ListOutputQueues => 3,
            Op::Attach
            | Op::SetOptions
            | Op::Prepare
            | Op::Execute
            | Op::Fetch
            | Op::CloseCursor
            | Op::Release
            | Op::Commit
            | Op::Rollback
            | Op::ListSchemas
            | Op::ListTables
            | Op::ListColumns
            | Op::JobInfo => 4,
            Op::ListDirectory
            | Op::GetInfo
            | Op::ReadFile
            | Op::WriteFile
            | Op::CreateDirectory
            | Op::Delete
            | Op::Rename
            | Op::Copy => 5,
            Op::ListMessages
            | Op::ReplyMessage
            | Op::RemoveMessages
            | Op::SendMessage
            | Op::WatchQueue
            | Op::UnwatchQueue
            | Op::WatchSpool
            | Op::UnwatchSpool
            | Op::Stat => 6,
            Op::ListSubsystems
            | Op::StartSubsystem
            | Op::EndSubsystem
            | Op::GetSystemValues
            | Op::SetSystemValue
            | Op::ListPtfs
            | Op::ListDiskUnits
            | Op::ListHardwareResources
            | Op::IplStatus
            | Op::PowerDown => 7,
            Op::GetLibraryList
            | Op::SetLibraryList
            | Op::ListLibraries
            | Op::ListObjects
            | Op::DescribeObject
            | Op::ListMembers
            | Op::ReadMember
            | Op::WriteMember
            | Op::UnlockMember
            | Op::CreateMember
            | Op::CopyMember
            | Op::RenameMember
            | Op::DeleteMember
            | Op::ChangeMember
            | Op::Compile
            | Op::ListJobs
            | Op::JobLog
            | Op::HoldJob
            | Op::ReleaseJob
            | Op::EndJob
            | Op::ListRoutines
            | Op::ListIndexes => 8,
            // Version 9: `Debug`'s operations => 9.
        }
    }

    /// Whether it runs in the connection's lane, one at a time in
    /// arrival order with the job's state: `Command` operations, `Sql`
    /// 1 to 8 (docs/QAP.md, Multiplexing) and, from version 8, the
    /// library list and `Compile` of `Develop`.
    #[must_use]
    pub const fn in_lane(self) -> bool {
        matches!(
            self,
            Op::RunCommand
                | Op::CheckCommand
                | Op::DescribeCommand
                | Op::GetLibraryList
                | Op::SetLibraryList
                | Op::Compile
                | Op::SetOptions
                | Op::Prepare
                | Op::Execute
                | Op::Fetch
                | Op::CloseCursor
                | Op::Release
                | Op::Commit
                | Op::Rollback
        )
    }

    /// The operation a service and code name.
    #[must_use]
    pub fn of(service: Service, code: u16) -> Option<Op> {
        Op::ALL.into_iter().find(|o| o.code() == (service, code))
    }

    /// Whether it is allowed before sign-on.
    #[must_use]
    pub const fn before_signon(self) -> bool {
        matches!(
            self,
            Op::Hello
                | Op::Ping
                | Op::SignOn
                | Op::ChangePassword
                | Op::Resume
                | Op::Attach
                | Op::JobInfo
        )
    }
}

/// Event codes, server to client.
pub mod event {
    use crate::envelope::Service;

    /// `Connection` 100: the server is ending and closes the connection.
    pub const SERVER_ENDING: u16 = 100;
    /// `SignOn` 100: the session ended.
    pub const SESSION_ENDED: u16 = 100;
    /// `Messages` 100: a message arrived on a watched queue (version 6).
    pub const MESSAGE_ARRIVED: u16 = 100;
    /// `Spool` 100: a watched target's file changed (version 6).
    pub const SPOOLED_FILE_CHANGED: u16 = 100;

    /// The protocol version that defines event `op` of `service`;
    /// `None` when there is no such event.
    #[must_use]
    pub const fn since(service: Service, op: u16) -> Option<u16> {
        match (service, op) {
            (Service::Connection, SERVER_ENDING) | (Service::SignOn, SESSION_ENDED) => Some(1),
            (Service::Messages, MESSAGE_ARRIVED) | (Service::Spool, SPOOLED_FILE_CHANGED) => {
                Some(6)
            }
            _ => None,
        }
    }
}
