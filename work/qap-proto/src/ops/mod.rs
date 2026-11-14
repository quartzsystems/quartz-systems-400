// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The operations of each service, their codes and bodies. Bodies not
//! listed are empty (`()`).

pub mod command;
pub mod connection;
pub mod debug;
pub mod develop;
pub mod dirsrv;
pub mod ifs;
pub mod jobs;
pub mod manage;
pub mod messages;
pub mod netserver;
pub mod nfs;
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
    /// `Debug` 1: start a debug session (version 9).
    DebugStart,
    /// `Debug` 2: end it (version 9).
    DebugEnd,
    /// `Debug` 3: put a program under debug (version 9).
    AddProgram,
    /// `Debug` 4: a module's breakpoints (version 9).
    SetBreakpoints,
    /// `Debug` 5: go on from a stop (version 9).
    DebugResume,
    /// `Debug` 6: the stopped job's call stack (version 9).
    Stack,
    /// `Debug` 7: a frame's variables (version 9).
    Variables,
    /// `Debug` 8: evaluate at the stop (version 9).
    Eval,
    /// `Debug` 9: stop a user's jobs when they call a program (version 9).
    SetServiceEntry,
    /// `Debug` 10: remove a service entry point (version 9).
    RemoveServiceEntry,
    /// `NetServer` 1: shares, filtered (version 10).
    ListShares,
    /// `NetServer` 2: one share (version 10).
    GetShare,
    /// `NetServer` 3: add a share (version 10).
    AddShare,
    /// `NetServer` 4: change a share (version 10).
    ChangeShare,
    /// `NetServer` 5: remove a share (version 10).
    RemoveShare,
    /// `NetServer` 6: sessions and their connections (version 10).
    ListSessions,
    /// `NetServer` 7: end a session (version 10).
    EndSession,
    /// `NetServer` 8: the attributes, current and pending (version 10).
    GetServerAttributes,
    /// `NetServer` 9: make attributes pending (version 10).
    ChangeServerAttributes,
    /// `NetServer` 10: users disabled for `NetServer` (version 10).
    ListDisabledUsers,
    /// `NetServer` 11: enable a user for `NetServer` again (version 10).
    EnableUser,
    /// `NetServer` 12: start `NetServer` (version 10).
    StartServer,
    /// `NetServer` 13: end `NetServer` (version 10).
    EndServer,
    /// `Nfs` 1: the export table (version 11).
    ListExports,
    /// `Nfs` 2: one export (version 11).
    GetExport,
    /// `Nfs` 3: export a directory, or again with other options
    /// (version 11).
    PutExport,
    /// `Nfs` 4: end an export (version 11).
    RemoveExport,
    /// `Nfs` 5: export every entry of `/etc/exports` (version 11).
    ExportAll,
    /// `Nfs` 6: the daemon jobs (version 11).
    ListDaemons,
    /// `Nfs` 7: start daemons (version 11).
    StartDaemons,
    /// `Nfs` 8: end daemons (version 11).
    EndDaemons,
    /// `Nfs` 9: the mount table (version 11).
    ListMounts,
    /// `Nfs` 10: mount an NFS export (version 11).
    MountNfs,
    /// `Nfs` 11: unmount (version 11).
    Unmount,
    /// `Nfs` 12: the NFS options (version 11).
    GetNfsOptions,
    /// `Nfs` 13: change the NFS options (version 11).
    SetNfsOptions,
    /// `Dirsrv` 1: the instances (version 12).
    ListDirInstances,
    /// `Dirsrv` 2: whether an instance starts automatically (version 12).
    SetDirAutostart,
    /// `Dirsrv` 3: start an instance (version 12).
    StartDirInstance,
    /// `Dirsrv` 4: end an instance (version 12).
    EndDirInstance,
    /// `Dirsrv` 5: end and start an instance (version 12).
    RestartDirInstance,
    /// `Dirsrv` 6: an instance's properties (version 12).
    GetDirProperties,
    /// `Dirsrv` 7: change an instance's properties (version 12).
    SetDirProperties,
    /// `Dirsrv` 8: an instance's suffixes (version 12).
    ListDirSuffixes,
    /// `Dirsrv` 9: add a user suffix (version 12).
    AddDirSuffix,
    /// `Dirsrv` 10: remove a user suffix (version 12).
    RemoveDirSuffix,
    /// `Dirsrv` 11: an instance's indexes (version 12).
    ListDirIndexes,
    /// `Dirsrv` 12: add or change an index (version 12).
    PutDirIndex,
    /// `Dirsrv` 13: remove an index (version 12).
    RemoveDirIndex,
    /// `Dirsrv` 14: the administrator and the update DN (version 12).
    GetDirAdmin,
    /// `Dirsrv` 15: change the administrator or a password (version 12).
    SetDirAdmin,
    /// `Dirsrv` 16: schema definitions (version 12).
    ListDirSchema,
    /// `Dirsrv` 17: add a schema definition (version 12).
    AddDirSchema,
    /// `Dirsrv` 18: remove a schema definition (version 12).
    RemoveDirSchema,
    /// `Dirsrv` 19: an entry's access control (version 12).
    GetDirAccess,
    /// `Dirsrv` 20: change an entry's access control (version 12).
    SetDirAccess,
    /// `Dirsrv` 21: the publishing agents (version 12).
    GetPublishing,
    /// `Dirsrv` 22: change a publishing agent (version 12).
    SetPublishing,
    /// `Dirsrv` 23: the publishing agents' status (version 12).
    GetPublishingStatus,
    /// `Dirsrv` 24: an instance's counters (version 12).
    GetDirCounters,
}

impl Op {
    /// Every operation.
    pub const ALL: [Op; 141] = [
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
        Op::DebugStart,
        Op::DebugEnd,
        Op::AddProgram,
        Op::SetBreakpoints,
        Op::DebugResume,
        Op::Stack,
        Op::Variables,
        Op::Eval,
        Op::SetServiceEntry,
        Op::RemoveServiceEntry,
        Op::ListShares,
        Op::GetShare,
        Op::AddShare,
        Op::ChangeShare,
        Op::RemoveShare,
        Op::ListSessions,
        Op::EndSession,
        Op::GetServerAttributes,
        Op::ChangeServerAttributes,
        Op::ListDisabledUsers,
        Op::EnableUser,
        Op::StartServer,
        Op::EndServer,
        Op::ListExports,
        Op::GetExport,
        Op::PutExport,
        Op::RemoveExport,
        Op::ExportAll,
        Op::ListDaemons,
        Op::StartDaemons,
        Op::EndDaemons,
        Op::ListMounts,
        Op::MountNfs,
        Op::Unmount,
        Op::GetNfsOptions,
        Op::SetNfsOptions,
        Op::ListDirInstances,
        Op::SetDirAutostart,
        Op::StartDirInstance,
        Op::EndDirInstance,
        Op::RestartDirInstance,
        Op::GetDirProperties,
        Op::SetDirProperties,
        Op::ListDirSuffixes,
        Op::AddDirSuffix,
        Op::RemoveDirSuffix,
        Op::ListDirIndexes,
        Op::PutDirIndex,
        Op::RemoveDirIndex,
        Op::GetDirAdmin,
        Op::SetDirAdmin,
        Op::ListDirSchema,
        Op::AddDirSchema,
        Op::RemoveDirSchema,
        Op::GetDirAccess,
        Op::SetDirAccess,
        Op::GetPublishing,
        Op::SetPublishing,
        Op::GetPublishingStatus,
        Op::GetDirCounters,
    ];

    /// Its service and code.
    #[must_use]
    #[allow(clippy::too_many_lines)] // one arm per operation
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
            Op::DebugStart => (Service::Debug, 1),
            Op::DebugEnd => (Service::Debug, 2),
            Op::AddProgram => (Service::Debug, 3),
            Op::SetBreakpoints => (Service::Debug, 4),
            Op::DebugResume => (Service::Debug, 5),
            Op::Stack => (Service::Debug, 6),
            Op::Variables => (Service::Debug, 7),
            Op::Eval => (Service::Debug, 8),
            Op::SetServiceEntry => (Service::Debug, 9),
            Op::RemoveServiceEntry => (Service::Debug, 10),
            Op::ListShares => (Service::NetServer, 1),
            Op::GetShare => (Service::NetServer, 2),
            Op::AddShare => (Service::NetServer, 3),
            Op::ChangeShare => (Service::NetServer, 4),
            Op::RemoveShare => (Service::NetServer, 5),
            Op::ListSessions => (Service::NetServer, 6),
            Op::EndSession => (Service::NetServer, 7),
            Op::GetServerAttributes => (Service::NetServer, 8),
            Op::ChangeServerAttributes => (Service::NetServer, 9),
            Op::ListDisabledUsers => (Service::NetServer, 10),
            Op::EnableUser => (Service::NetServer, 11),
            Op::StartServer => (Service::NetServer, 12),
            Op::EndServer => (Service::NetServer, 13),
            Op::ListExports => (Service::Nfs, 1),
            Op::GetExport => (Service::Nfs, 2),
            Op::PutExport => (Service::Nfs, 3),
            Op::RemoveExport => (Service::Nfs, 4),
            Op::ExportAll => (Service::Nfs, 5),
            Op::ListDaemons => (Service::Nfs, 6),
            Op::StartDaemons => (Service::Nfs, 7),
            Op::EndDaemons => (Service::Nfs, 8),
            Op::ListMounts => (Service::Nfs, 9),
            Op::MountNfs => (Service::Nfs, 10),
            Op::Unmount => (Service::Nfs, 11),
            Op::GetNfsOptions => (Service::Nfs, 12),
            Op::SetNfsOptions => (Service::Nfs, 13),
            Op::ListDirInstances => (Service::Dirsrv, 1),
            Op::SetDirAutostart => (Service::Dirsrv, 2),
            Op::StartDirInstance => (Service::Dirsrv, 3),
            Op::EndDirInstance => (Service::Dirsrv, 4),
            Op::RestartDirInstance => (Service::Dirsrv, 5),
            Op::GetDirProperties => (Service::Dirsrv, 6),
            Op::SetDirProperties => (Service::Dirsrv, 7),
            Op::ListDirSuffixes => (Service::Dirsrv, 8),
            Op::AddDirSuffix => (Service::Dirsrv, 9),
            Op::RemoveDirSuffix => (Service::Dirsrv, 10),
            Op::ListDirIndexes => (Service::Dirsrv, 11),
            Op::PutDirIndex => (Service::Dirsrv, 12),
            Op::RemoveDirIndex => (Service::Dirsrv, 13),
            Op::GetDirAdmin => (Service::Dirsrv, 14),
            Op::SetDirAdmin => (Service::Dirsrv, 15),
            Op::ListDirSchema => (Service::Dirsrv, 16),
            Op::AddDirSchema => (Service::Dirsrv, 17),
            Op::RemoveDirSchema => (Service::Dirsrv, 18),
            Op::GetDirAccess => (Service::Dirsrv, 19),
            Op::SetDirAccess => (Service::Dirsrv, 20),
            Op::GetPublishing => (Service::Dirsrv, 21),
            Op::SetPublishing => (Service::Dirsrv, 22),
            Op::GetPublishingStatus => (Service::Dirsrv, 23),
            Op::GetDirCounters => (Service::Dirsrv, 24),
        }
    }

    /// The protocol version that defines it.
    #[must_use]
    #[allow(clippy::too_many_lines)] // one arm per operation
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
            Op::DebugStart
            | Op::DebugEnd
            | Op::AddProgram
            | Op::SetBreakpoints
            | Op::DebugResume
            | Op::Stack
            | Op::Variables
            | Op::Eval
            | Op::SetServiceEntry
            | Op::RemoveServiceEntry => 9,
            Op::ListShares
            | Op::GetShare
            | Op::AddShare
            | Op::ChangeShare
            | Op::RemoveShare
            | Op::ListSessions
            | Op::EndSession
            | Op::GetServerAttributes
            | Op::ChangeServerAttributes
            | Op::ListDisabledUsers
            | Op::EnableUser
            | Op::StartServer
            | Op::EndServer => 10,
            Op::ListExports
            | Op::GetExport
            | Op::PutExport
            | Op::RemoveExport
            | Op::ExportAll
            | Op::ListDaemons
            | Op::StartDaemons
            | Op::EndDaemons
            | Op::ListMounts
            | Op::MountNfs
            | Op::Unmount
            | Op::GetNfsOptions
            | Op::SetNfsOptions => 11,
            Op::ListDirInstances
            | Op::SetDirAutostart
            | Op::StartDirInstance
            | Op::EndDirInstance
            | Op::RestartDirInstance
            | Op::GetDirProperties
            | Op::SetDirProperties
            | Op::ListDirSuffixes
            | Op::AddDirSuffix
            | Op::RemoveDirSuffix
            | Op::ListDirIndexes
            | Op::PutDirIndex
            | Op::RemoveDirIndex
            | Op::GetDirAdmin
            | Op::SetDirAdmin
            | Op::ListDirSchema
            | Op::AddDirSchema
            | Op::RemoveDirSchema
            | Op::GetDirAccess
            | Op::SetDirAccess
            | Op::GetPublishing
            | Op::SetPublishing
            | Op::GetPublishingStatus
            | Op::GetDirCounters => 12,
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
    /// `Debug` 100: a debugged job stopped (version 9).
    pub const STOPPED: u16 = 100;
    /// `Debug` 101: the debug session ended (version 9).
    pub const DEBUG_ENDED: u16 = 101;

    /// The protocol version that defines event `op` of `service`;
    /// `None` when there is no such event.
    #[must_use]
    pub const fn since(service: Service, op: u16) -> Option<u16> {
        match (service, op) {
            (Service::Connection, SERVER_ENDING) | (Service::SignOn, SESSION_ENDED) => Some(1),
            (Service::Messages, MESSAGE_ARRIVED) | (Service::Spool, SPOOLED_FILE_CHANGED) => {
                Some(6)
            }
            (Service::Debug, STOPPED | DEBUG_ENDED) => Some(9),
            _ => None,
        }
    }
}
