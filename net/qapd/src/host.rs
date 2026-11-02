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

use qap_proto::ops::command::{CommandDefinition, CommandReply};
use qap_proto::ops::signon::{PasswordStatus, SignOnReply, TelnetTicketReply};
use qap_proto::ops::spool::{
    ListOutputQueues, ListSpooledFiles, OutputQueues, ReadSpooledFile, SpooledData, SpooledFileId, SpooledFiles,
};
use qap_proto::ops::sql::{
    Columns, Execute, Executed, Fetch, Fetched, ListColumns, ListSchemas, ListTables, Prepared, Schemas, SetOptions,
    Tables,
};
use qap_proto::ops::system::SystemInfo;
use qap_proto::types::ObjectName;
use qap_proto::{Secret, ServerMessage, Status, Token};

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
        Err(SqlStop::Refused(no_database(self.message(NO_DATABASE, &["Prepare"]))))
    }
    /// Run a prepared statement once per parameter row, stopping at the
    /// first that fails (`SqlFailure.row`); a query opens a cursor.
    /// `cancelled` as for [`Host::run_command`].
    fn sql_execute(&mut self, _req: &Execute, _cancelled: &dyn Fn() -> bool) -> SqlResult<Executed> {
        Err(SqlStop::Refused(no_database(self.message(NO_DATABASE, &["Execute"]))))
    }
    /// Rows from a cursor, at most `max_rows` (0 for all).
    fn sql_fetch(&mut self, _req: &Fetch, _cancelled: &dyn Fn() -> bool) -> SqlResult<Fetched> {
        Err(SqlStop::Refused(no_database(self.message(NO_DATABASE, &["Fetch"]))))
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
        Err(SqlStop::Refused(no_database(self.message(NO_DATABASE, &["Commit"]))))
    }
    /// Roll back the connection's transaction.
    fn sql_rollback(&mut self) -> SqlResult<()> {
        Err(SqlStop::Refused(no_database(self.message(NO_DATABASE, &["Rollback"]))))
    }
    /// The connection ends or runs as another profile: changes not
    /// committed are rolled back, its statements and cursors go.
    fn sql_end(&mut self) {}
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
    fn change_password(&mut self, user: &str, old: &Secret, new: &Secret) -> Result<SignOnReply, Refusal>;
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
    fn move_spooled_file(&mut self, id: &SpooledFileId, outq: &ObjectName, first: bool) -> Result<(), Refusal>;
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
}

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
