// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What a connection reaches of the system: the connection job's side.
//! [`Host`] is the job's own thread, where commands run one at a time
//! with the job's state; [`ThreadHost`] is a secondary thread of the
//! same job, where every other operation runs, as the same profile.
//! The job implements them over its links to the storage service;
//! tests implement them in memory.

use std::sync::Arc;

use ifs::client::IfsClient;
use qap_proto::ops::command::{CommandDefinition, CommandReply};
use qap_proto::ops::develop::{
    ChangeMember, CopyMember, CreateMember, DescribeObject, Library, LibraryList, ListLibraries,
    ListMembers, ListObjects, MemberData, MemberEntry, ObjectDescription, ObjectEntry,
    RenameMember, SetLibraryList,
};
use qap_proto::ops::jobs::{EndJob, JobEntry, JobLogEntry, ListJobs};
use qap_proto::ops::manage::{
    DiskUnits, EndSubsystem, HardwareResources, IplStatus, PowerDown, Ptf, ResourceType,
    SetSystemValue, Subsystem, SystemValue, SystemValues,
};
use qap_proto::ops::messages::{
    ListMessages, Messages, RemoveMessages, ReplyMessage, SendMessage, Sent,
};
use qap_proto::ops::signon::{PasswordStatus, SignOnReply, TelnetTicketReply};
use qap_proto::ops::spool::{
    ListOutputQueues, ListSpooledFiles, OutputQueues, ReadSpooledFile, SpooledData, SpooledFileId,
    SpooledFiles,
};
use qap_proto::ops::sql::{
    Columns, Execute, Executed, Fetch, Fetched, Indexes, ListColumns, ListIndexes, ListRoutines,
    ListSchemas, ListTables, Prepared, Routine, Schemas, SetOptions, Tables,
};
use qap_proto::ops::system::SystemInfo;
use qap_proto::types::{JobId, MemberName, ObjectName, SourceRecord};
use qap_proto::{Secret, ServerMessage, Status, Token};

use crate::develop::{Compiled, NOT_AVAILABLE};
use crate::manage::ExitRequest;
use crate::sessions::EndedBy;
use crate::sql::{SqlResult, SqlStop};

/// Why the connection is to end, from outside it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ending {
    /// `ENDTCPSVR` or the subsystem: `ServerEnding`, then close.
    Server,
    /// An operator ended the job: `SessionEnded` when signed on, then
    /// close.
    Operator,
    /// Another connection of the session ended it: `SessionEnded`, then
    /// close.
    Session(EndedBy),
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

/// Makes a secondary thread's reach; called on that thread, once.
pub type Threads = Arc<dyn Fn() -> Box<dyn ThreadHost> + Send + Sync>;

/// The connection job's own thread: commands, in arrival order, with
/// the job's library list, `QTEMP` and attributes; and the job's end.
pub trait Host {
    /// Message `id` with substitution data `data`, its text in the
    /// language of the job's user now: the system's before sign-on, the
    /// signed-on profile's after it. The connection asks while the job
    /// still runs as the user a message is for.
    fn message(&mut self, id: &str, data: &[&str]) -> ServerMessage;
    /// Whether the connection is to end.
    fn ending(&mut self) -> Option<Ending>;
    /// End the session; the job runs as its own user again.
    fn sign_off(&mut self, token: &Token);
    /// An operator ended the job: end its session, its other
    /// connections told so.
    fn end_session(&mut self, token: &Token) {
        self.sign_off(token);
    }
    /// The connection closed: the session waits to be resumed.
    fn detach(&mut self, token: &Token);
    /// The job runs as another profile since the last command: a
    /// secondary thread signed on, resumed or signed off.
    fn swapped(&mut self) {}

    /// Run a CL command as the signed-on profile; every message it sent
    /// its caller, oldest first. `cancelled` says whether the client has
    /// cancelled this request, by its ID on this connection; it stays
    /// true once it is.
    fn run_command(&mut self, command: &str, cancelled: &dyn Fn() -> bool) -> Ran;
    /// Check a CL command without running it: `Ok` with no messages
    /// when valid, else `Error` (or `NotFound`) with the check's.
    fn check_command(&mut self, command: &str) -> (Status, Vec<ServerMessage>);
    /// A command's definition as the prompter has it: `NotFound` or
    /// `NotAuthorized` with the message why when it cannot be had.
    fn describe_command(&mut self, command: &ObjectName) -> Result<CommandDefinition, Refusal>;
    /// What ends a running command when the client cancels it.
    fn canceller(&mut self) -> Option<Canceller> {
        None
    }

    /// What makes the job's secondary threads.
    fn threads(&mut self) -> Threads;

    /// `SetOptions`: commitment control, naming and default schema for
    /// the connection's next statements; `InUse` while a transaction is
    /// open. Every `sql_` method answers [`no_database`] unless the host
    /// has one.
    fn sql_set_options(&mut self, _options: &SetOptions) -> Result<(), Refusal> {
        Err(no_database(self.message(NO_DATABASE, &["SetOptions"])))
    }
    /// Prepare a statement as the signed-on profile.
    fn sql_prepare(&mut self, _sql: &str) -> SqlResult<Prepared> {
        Err(SqlStop::Refused(no_database(
            self.message(NO_DATABASE, &["Prepare"]),
        )))
    }
    /// Run a prepared statement once per parameter row, stopping at the
    /// first that fails (`SqlFailure.row`); a query opens a cursor.
    /// `cancelled` as for [`Host::run_command`].
    fn sql_execute(
        &mut self,
        _req: &Execute,
        _cancelled: &dyn Fn() -> bool,
    ) -> SqlResult<Executed> {
        Err(SqlStop::Refused(no_database(
            self.message(NO_DATABASE, &["Execute"]),
        )))
    }
    /// Rows from a cursor, at most `max_rows` (0 for all).
    fn sql_fetch(&mut self, _req: &Fetch, _cancelled: &dyn Fn() -> bool) -> SqlResult<Fetched> {
        Err(SqlStop::Refused(no_database(
            self.message(NO_DATABASE, &["Fetch"]),
        )))
    }
    /// Close a cursor; `NotFound` when it is not open.
    fn sql_close_cursor(&mut self, _cursor: u32) -> Result<(), Refusal> {
        Err(no_database(self.message(NO_DATABASE, &["CloseCursor"])))
    }
    /// Release a prepared statement; `NotFound` when there is none.
    fn sql_release(&mut self, _statement: u32) -> Result<(), Refusal> {
        Err(no_database(self.message(NO_DATABASE, &["Release"])))
    }
    /// Commit the connection's transaction.
    fn sql_commit(&mut self) -> SqlResult<()> {
        Err(SqlStop::Refused(no_database(
            self.message(NO_DATABASE, &["Commit"]),
        )))
    }
    /// Roll back the connection's transaction.
    fn sql_rollback(&mut self) -> SqlResult<()> {
        Err(SqlStop::Refused(no_database(
            self.message(NO_DATABASE, &["Rollback"]),
        )))
    }
    /// The connection ends or runs as another profile: changes not
    /// committed are rolled back, its statements and cursors go.
    fn sql_end(&mut self) {}

    /// `GetLibraryList` (version 8): the job's library list, as
    /// `DSPLIBL` shows it. Every `Develop` method answers `Error` with
    /// [`NOT_AVAILABLE`] unless the host has it.
    fn library_list(&mut self) -> Result<LibraryList, Refusal> {
        Err(no_database(
            self.message(NOT_AVAILABLE, &["GetLibraryList"]),
        ))
    }
    /// `SetLibraryList`: `CHGLIBL LIBL(user) CURLIB(current)` (`*NONE`
    /// and `*CRTDFT` when empty); the list as it now is.
    fn set_library_list(&mut self, _req: &SetLibraryList) -> Result<LibraryList, Refusal> {
        Err(no_database(
            self.message(NOT_AVAILABLE, &["SetLibraryList"]),
        ))
    }
    /// `Compile`: run `command` as [`Host::run_command`] does and take
    /// the diagnostics the compiler left on the job for it.
    fn compile(
        &mut self,
        _command: &str,
        _cancelled: &dyn Fn() -> bool,
    ) -> Result<Compiled, Refusal> {
        Err(no_database(self.message(NOT_AVAILABLE, &["Compile"])))
    }
}

/// A secondary thread of the connection job: every operation but
/// commands, as the job's profile, without waiting for a command.
pub trait ThreadHost {
    /// The system's name, release, time zone and CCSID.
    fn system_id(&mut self) -> SystemId;
    /// Milliseconds since 1970-01-01 UTC.
    fn now_ms(&mut self) -> i64;
    /// As [`Host::message`].
    fn message(&mut self, id: &str, data: &[&str]) -> ServerMessage;
    /// The system's description and installed products.
    fn system_info(&mut self) -> SystemInfo;

    /// Sign on as a 5250 display would; on success the job runs as the
    /// user and a session exists.
    fn sign_on(&mut self, user: &str, password: &Secret) -> Result<SignOnReply, Refusal>;
    /// Change `user`'s password; signed on as [`ThreadHost::sign_on`]
    /// when the connection was not. In a signed-on connection the
    /// reply's token is all zeros: the session goes on with its own.
    fn change_password(
        &mut self,
        user: &str,
        old: &Secret,
        new: &Secret,
    ) -> Result<SignOnReply, Refusal>;
    /// Move the session `token` names to this connection.
    fn resume(&mut self, token: &Token) -> Result<SignOnReply, Refusal>;
    /// Attach this connection to the session `token` names beside its
    /// others (version 4), the job running as its profile with job
    /// state of its own; `SessionExpired` or `Busy` with the message why.
    fn attach(&mut self, token: &Token) -> Result<SignOnReply, Refusal>;
    /// End the session; the job runs as its own user again.
    fn sign_off(&mut self, token: &Token);
    /// When the signed-on profile's password expires.
    fn password_status(&mut self) -> PasswordStatus;
    /// A one-time ticket for 5250 automatic sign-on as the session's
    /// profile; `NotAuthorized` with the message why when it may not.
    fn telnet_ticket(&mut self) -> Result<TelnetTicketReply, Refusal>;

    /// Spooled files, as `WRKSPLF` lists them, filtered and paged.
    fn list_spooled_files(&mut self, req: &ListSpooledFiles) -> Result<SpooledFiles, Refusal>;
    /// Pages of a spooled file ([`crate::spool::read`]), checked and
    /// audited as `DSPSPLF` reads it.
    fn read_spooled_file(&mut self, req: &ReadSpooledFile) -> Result<SpooledData, Refusal>;
    /// Hold a spooled file as `HLDSPLF` does; `immediately` is
    /// `OPTION(*IMMED)`, else `*PAGEEND`.
    fn hold_spooled_file(&mut self, id: &SpooledFileId, immediately: bool) -> Result<(), Refusal>;
    /// Release a spooled file as `RLSSPLF` does.
    fn release_spooled_file(&mut self, id: &SpooledFileId) -> Result<(), Refusal>;
    /// Delete a spooled file as `DLTSPLF` does.
    fn delete_spooled_file(&mut self, id: &SpooledFileId) -> Result<(), Refusal>;
    /// Move a spooled file to `outq` as `CHGSPLFA OUTQ` does; `first` to
    /// the top of the queue.
    fn move_spooled_file(
        &mut self,
        id: &SpooledFileId,
        outq: &ObjectName,
        first: bool,
    ) -> Result<(), Refusal>;
    /// Output queues, as `WRKOUTQ` lists them, paged.
    fn list_output_queues(&mut self, req: &ListOutputQueues) -> Result<OutputQueues, Refusal>;

    /// Schemas in the catalog, by `LIKE` pattern, paged.
    fn list_schemas(&mut self, _req: &ListSchemas) -> Result<Schemas, Refusal> {
        Err(no_database(self.message(NO_DATABASE, &["ListSchemas"])))
    }
    /// Tables, views, aliases and files of a schema, paged.
    fn list_tables(&mut self, _req: &ListTables) -> Result<Tables, Refusal> {
        Err(no_database(self.message(NO_DATABASE, &["ListTables"])))
    }
    /// A table's columns, in order.
    fn list_columns(&mut self, _req: &ListColumns) -> Result<Columns, Refusal> {
        Err(no_database(self.message(NO_DATABASE, &["ListColumns"])))
    }

    /// Run `f` with the integrated file system as the job's profile sees
    /// it: the client the job's file-system commands use, so the `Ifs`
    /// service ([`crate::files`]) has their authority checks and audit
    /// entries. A host without one answers `Error` with [`NO_IFS`].
    fn with_ifs(&mut self, _f: &mut dyn FnMut(&dyn IfsClient)) -> Result<(), Refusal> {
        Err(no_database(self.message(NO_IFS, &[])))
    }
    /// Message `id` with substitution data `data` laid out as its
    /// variables are (a command's message), its text in the job's
    /// language as [`ThreadHost::message`]'s.
    fn message_data(&mut self, id: &str, data: &[u8]) -> ServerMessage {
        let mut m = self.message(id, &[]);
        m.data = data.to_vec();
        m
    }

    /// A queue's messages as `DSPMSG` reads them, in arrival order
    /// (strictly newest first when asked), paged, their text in the
    /// job's language. Watches list through it too. Every message
    /// method answers `Error` with [`NO_MESSAGES`] unless the host has
    /// message queues.
    fn list_messages(&mut self, _req: &ListMessages) -> Result<Messages, Refusal> {
        Err(no_database(self.message(NO_MESSAGES, &[])))
    }
    /// Answer an inquiry as `SNDRPYMSG` does, the reply checked against
    /// the message's valid replies.
    fn reply_message(&mut self, _req: &ReplyMessage) -> Result<(), Refusal> {
        Err(no_database(self.message(NO_MESSAGES, &[])))
    }
    /// Remove messages as `RMVMSG` does: all, or none when a key is not
    /// on the queue.
    fn remove_messages(&mut self, _req: &RemoveMessages) -> Result<(), Refusal> {
        Err(no_database(self.message(NO_MESSAGES, &[])))
    }
    /// Send a message as `SNDMSG` does; an inquiry's reply comes to the
    /// profile's own queue.
    fn send_message(&mut self, _req: &SendMessage) -> Result<Sent, Refusal> {
        Err(no_database(self.message(NO_MESSAGES, &[])))
    }

    /// Ask the exit programs registered on `point` with `format`
    /// whether `req` may run (version 7); a refusal is the status and
    /// message the client gets (`NotAuthorized`, [`crate::manage::EXIT_REFUSED`]).
    /// A host without exit programs allows everything.
    fn exit_allows(
        &mut self,
        _point: &str,
        _format: &str,
        _req: &ExitRequest,
    ) -> Result<(), Refusal> {
        Ok(())
    }
    /// Subsystem descriptions of `library` (`*ALL`, `*LIBL`) and their
    /// states, as `WRKSBS` lists them: active first, then by name. Every
    /// management method answers `Error` with [`crate::manage::NO_MANAGE`]
    /// unless the host has them.
    fn subsystems(&mut self, _library: &str) -> Result<Vec<Subsystem>, Refusal> {
        Err(no_database(self.message(crate::manage::NO_MANAGE, &[])))
    }
    /// `STRSBS`.
    fn start_subsystem(&mut self, _sbsd: &ObjectName) -> Result<(), Refusal> {
        Err(no_database(self.message(crate::manage::NO_MANAGE, &[])))
    }
    /// `ENDSBS`.
    fn end_subsystem(&mut self, _req: &EndSubsystem) -> Result<(), Refusal> {
        Err(no_database(self.message(crate::manage::NO_MANAGE, &[])))
    }
    /// System values by name (all when `names` is empty), as
    /// `RTVSYSVAL` reads them.
    fn system_values(&mut self, _names: &[String]) -> Result<SystemValues, Refusal> {
        Err(no_database(self.message(crate::manage::NO_MANAGE, &[])))
    }
    /// `CHGSYSVAL`; the value as it now is.
    fn set_system_value(&mut self, _req: &SetSystemValue) -> Result<SystemValue, Refusal> {
        Err(no_database(self.message(crate::manage::NO_MANAGE, &[])))
    }
    /// PTFs of `product` (`*ALL`), as `DSPPTF` lists them.
    fn ptfs(&mut self, _product: &str) -> Result<Vec<Ptf>, Refusal> {
        Err(no_database(self.message(crate::manage::NO_MANAGE, &[])))
    }
    /// Disk units and ASPs, as `WRKDSKSTS` and DST show them.
    fn disk_units(&mut self) -> Result<DiskUnits, Refusal> {
        Err(no_database(self.message(crate::manage::NO_MANAGE, &[])))
    }
    /// Hardware resources of a type, as `WRKHDWRSC` lists them.
    fn hardware_resources(&mut self, _kind: ResourceType) -> Result<HardwareResources, Refusal> {
        Err(no_database(self.message(crate::manage::NO_MANAGE, &[])))
    }
    /// The IPL's state and the last SRC.
    fn ipl_status(&mut self) -> Result<IplStatus, Refusal> {
        Err(no_database(self.message(crate::manage::NO_MANAGE, &[])))
    }
    /// `PWRDWNSYS`: started once answered; the connection then ends
    /// with the server.
    fn power_down(&mut self, _req: &PowerDown) -> Result<(), Refusal> {
        Err(no_database(self.message(crate::manage::NO_MANAGE, &[])))
    }

    // `Develop` 3 to 14 (version 8): each as its command, as the job's
    // profile; `Error` with `NOT_AVAILABLE` unless the host has them.
    // Lists come whole, filtered and in order; the server pages them.

    /// Libraries matching `req.pattern`, as `WRKLIB` lists them.
    fn list_libraries(&mut self, _req: &ListLibraries) -> Result<Vec<Library>, Refusal> {
        Err(no_database(self.message(NOT_AVAILABLE, &["ListLibraries"])))
    }
    /// Objects, as `WRKOBJ` lists them.
    fn list_objects(&mut self, _req: &ListObjects) -> Result<Vec<ObjectEntry>, Refusal> {
        Err(no_database(self.message(NOT_AVAILABLE, &["ListObjects"])))
    }
    /// An object's description, as `DSPOBJD DETAIL(*FULL)`.
    fn describe_object(&mut self, _req: &DescribeObject) -> Result<ObjectDescription, Refusal> {
        Err(no_database(
            self.message(NOT_AVAILABLE, &["DescribeObject"]),
        ))
    }
    /// A file's members matching `req.pattern`, by name.
    fn list_members(&mut self, _req: &ListMembers) -> Result<Vec<MemberEntry>, Refusal> {
        Err(no_database(self.message(NOT_AVAILABLE, &["ListMembers"])))
    }
    /// A source member's records; with `lock`, held `*SHRNUP` for the
    /// connection until [`ThreadHost::unlock_member`], a write of it or
    /// the connection's end.
    fn read_member(&mut self, _member: &MemberName, _lock: bool) -> Result<MemberData, Refusal> {
        Err(no_database(self.message(NOT_AVAILABLE, &["ReadMember"])))
    }
    /// Replace a member's records with `records` (every piece, their
    /// sequence numbers checked), dated by [`crate::develop::dated`]
    /// and converted to the file's CCSID; `srctype` and `text` when
    /// given. The member's records written; the connection's lock ends.
    fn write_member(
        &mut self,
        _member: &MemberName,
        _records: Vec<SourceRecord>,
        _renumber: bool,
        _srctype: Option<&str>,
        _text: Option<&str>,
    ) -> Result<u32, Refusal> {
        Err(no_database(self.message(NOT_AVAILABLE, &["WriteMember"])))
    }
    /// End the connection's lock on a member; `NotFound` with
    /// [`crate::develop::NOT_LOCKED`] when it holds none.
    fn unlock_member(&mut self, _member: &MemberName) -> Result<(), Refusal> {
        Err(no_database(self.message(NOT_AVAILABLE, &["UnlockMember"])))
    }
    /// `ADDPFM`.
    fn create_member(&mut self, _req: &CreateMember) -> Result<(), Refusal> {
        Err(no_database(self.message(NOT_AVAILABLE, &["CreateMember"])))
    }
    /// `CPYSRCF` of one member.
    fn copy_member(&mut self, _req: &CopyMember) -> Result<(), Refusal> {
        Err(no_database(self.message(NOT_AVAILABLE, &["CopyMember"])))
    }
    /// `RNMM`.
    fn rename_member(&mut self, _req: &RenameMember) -> Result<(), Refusal> {
        Err(no_database(self.message(NOT_AVAILABLE, &["RenameMember"])))
    }
    /// `RMVM`.
    fn delete_member(&mut self, _member: &MemberName) -> Result<(), Refusal> {
        Err(no_database(self.message(NOT_AVAILABLE, &["DeleteMember"])))
    }
    /// `CHGPFM SRCTYPE() TEXT()`.
    fn change_member(&mut self, _req: &ChangeMember) -> Result<(), Refusal> {
        Err(no_database(self.message(NOT_AVAILABLE, &["ChangeMember"])))
    }

    // `Jobs` (version 8).

    /// Jobs, as `WRKUSRJOB` lists them: active, on job queues, on
    /// output queues, each by when it entered.
    fn list_jobs(&mut self, _req: &ListJobs) -> Result<Vec<JobEntry>, Refusal> {
        Err(no_database(self.message(NOT_AVAILABLE, &["ListJobs"])))
    }
    /// A job's log, oldest first, as `DSPJOBLOG`; an ended job's from
    /// its spooled job log.
    fn job_log(&mut self, _job: &JobId) -> Result<Vec<JobLogEntry>, Refusal> {
        Err(no_database(self.message(NOT_AVAILABLE, &["JobLog"])))
    }
    /// `HLDJOB`.
    fn hold_job(&mut self, _job: &JobId) -> Result<(), Refusal> {
        Err(no_database(self.message(NOT_AVAILABLE, &["HoldJob"])))
    }
    /// `RLSJOB`.
    fn release_job(&mut self, _job: &JobId) -> Result<(), Refusal> {
        Err(no_database(self.message(NOT_AVAILABLE, &["ReleaseJob"])))
    }
    /// `ENDJOB`.
    fn end_job(&mut self, _req: &EndJob) -> Result<(), Refusal> {
        Err(no_database(self.message(NOT_AVAILABLE, &["EndJob"])))
    }

    /// `Sql` 12 (version 8): a schema's procedures and functions from
    /// the catalog, filtered, by name then specific name.
    fn list_routines(&mut self, _req: &ListRoutines) -> Result<Vec<Routine>, Refusal> {
        Err(no_database(self.message(NO_DATABASE, &["ListRoutines"])))
    }
    /// `Sql` 13: a table's indexes and their keys, by name.
    fn list_indexes(&mut self, _req: &ListIndexes) -> Result<Indexes, Refusal> {
        Err(no_database(self.message(NO_DATABASE, &["ListIndexes"])))
    }

    /// The source debugger (version 9); `None` for a host without one,
    /// whose `Debug` operations answer `Error` with [`crate::debug::NO_DEBUG`].
    fn debugger(&mut self) -> Option<&mut dyn crate::debug::DebugHost> {
        None
    }

    /// `NetServer`'s APIs, sessions and disabled users (version 10); `None`
    /// for a host without them, whose `NetServer` operations answer
    /// `Error` with [`crate::manage::NO_MANAGE`].
    fn netserver(&mut self) -> Option<&mut dyn crate::netserver::NetServerHost> {
        None
    }
}

/// The message of a `Messages` operation a host without message queues
/// answers.
pub const NO_MESSAGES: &str = "TCP1A5C";

/// What `UnwatchQueue` and `UnwatchSpool` answer for something not
/// watched on the connection.
pub const NOT_WATCHED: &str = "TCP1A5B";

/// The message of an `Ifs` operation a host without a file system
/// answers: the function is not supported.
pub const NO_IFS: &str = "CPFA0AD";

/// The message of an `Sql` operation a host without a database answers.
pub const NO_DATABASE: &str = "SQL0084";

/// The refusal of a host without a database: `Error`, `message`.
#[must_use]
pub fn no_database(message: ServerMessage) -> Refusal {
    Refusal {
        status: Status::Error,
        message,
    }
}
