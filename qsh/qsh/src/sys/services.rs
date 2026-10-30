// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What the utilities reach past the file system for: the name
//! resolver ([`Resolver`]), CL commands ([`ClRunner`]), SQL
//! ([`SqlRunner`]) and the IBM i objects Qshell's own utilities read
//! and change (library list, system values, data areas and queues,
//! spooled files, IPC objects, attributes). `Sys` reaches each through
//! a method that defaults to `ENOSYS`.

use std::time::Duration;

use super::SysError;

/// DNS record types by number.
pub mod rtype {
    /// An IPv4 address.
    pub const A: u16 = 1;
    /// A name server.
    pub const NS: u16 = 2;
    /// A canonical name.
    pub const CNAME: u16 = 5;
    /// The start of a zone of authority.
    pub const SOA: u16 = 6;
    /// A domain name pointer.
    pub const PTR: u16 = 12;
    /// Host information.
    pub const HINFO: u16 = 13;
    /// A mail exchanger.
    pub const MX: u16 = 15;
    /// Text.
    pub const TXT: u16 = 16;
    /// An IPv6 address.
    pub const AAAA: u16 = 28;
    /// A service location.
    pub const SRV: u16 = 33;
    /// Every type.
    pub const ANY: u16 = 255;

    /// Types by name, for the utilities' `-type=` and `-t`.
    pub const NAMES: &[(&str, u16)] = &[
        ("A", A),
        ("NS", NS),
        ("CNAME", CNAME),
        ("SOA", SOA),
        ("PTR", PTR),
        ("HINFO", HINFO),
        ("MX", MX),
        ("TXT", TXT),
        ("AAAA", AAAA),
        ("SRV", SRV),
        ("ANY", ANY),
    ];

    /// The name of a type (`TYPEn` when it has none).
    pub fn name(t: u16) -> String {
        NAMES
            .iter()
            .find(|(_, n)| *n == t)
            .map_or_else(|| format!("TYPE{t}"), |(s, _)| (*s).to_string())
    }

    /// The type of a name, in any case (`TYPEn` too).
    pub fn parse(s: &str) -> Option<u16> {
        let up = s.to_ascii_uppercase();
        if let Some(n) = up.strip_prefix("TYPE") {
            return n.parse().ok();
        }
        NAMES.iter().find(|(n, _)| *n == up).map(|(_, t)| *t)
    }
}

/// DNS response codes.
pub mod rcode {
    /// No error.
    pub const NOERROR: u8 = 0;
    /// The server could not interpret the query.
    pub const FORMERR: u8 = 1;
    /// The server failed.
    pub const SERVFAIL: u8 = 2;
    /// The name does not exist.
    pub const NXDOMAIN: u8 = 3;
    /// Not implemented.
    pub const NOTIMP: u8 = 4;
    /// Refused.
    pub const REFUSED: u8 = 5;

    /// The code's name.
    pub fn name(c: u8) -> String {
        match c {
            NOERROR => "NOERROR".to_string(),
            FORMERR => "FORMERR".to_string(),
            SERVFAIL => "SERVFAIL".to_string(),
            NXDOMAIN => "NXDOMAIN".to_string(),
            NOTIMP => "NOTIMP".to_string(),
            REFUSED => "REFUSED".to_string(),
            n => format!("RCODE{n}"),
        }
    }
}

/// A question for the resolver.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DnsQuery {
    /// The name asked about, as the user gave it (the resolver applies
    /// its search list to a relative name).
    pub name: String,
    /// The record type ([`rtype`]).
    pub rtype: u16,
    /// A server to ask instead of the configured ones.
    pub server: Option<String>,
    /// Ask for recursion.
    pub recurse: bool,
}

/// One resource record. Names are absolute without a final dot; the
/// data is in presentation form, domain names in it ending in a dot
/// (`10 mail.example.com.`, `"text"`, the five SOA fields after the two
/// names).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DnsRecord {
    /// The owner name.
    pub name: String,
    /// The type.
    pub rtype: u16,
    /// Time to live, seconds.
    pub ttl: u32,
    /// The data.
    pub data: String,
}

/// What the resolver answered.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DnsAnswer {
    /// The server that answered (an address).
    pub server: String,
    /// Its port.
    pub port: u16,
    /// The response code ([`rcode`]).
    pub rcode: u8,
    /// The answer came from a server with authority for the zone.
    pub authoritative: bool,
    /// The name the question resolved to after the search list.
    pub qname: String,
    /// The answer section.
    pub answers: Vec<DnsRecord>,
    /// The authority section.
    pub authority: Vec<DnsRecord>,
    /// The additional section.
    pub additional: Vec<DnsRecord>,
}

/// The name resolver (`nslookup`, `host`, `dig`, `hostname`).
pub trait Resolver: Send + Sync {
    /// Ask a question.
    fn query(&self, q: &DnsQuery) -> Result<DnsAnswer, SysError>;
    /// The system's TCP/IP host name, qualified by its domain.
    fn host_name(&self) -> Result<String, SysError>;
}

/// A CL message's type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MsgKind {
    /// `*COMP`.
    Completion,
    /// `*INFO`.
    Informational,
    /// `*DIAG`.
    Diagnostic,
    /// `*ESCAPE`.
    Escape,
    /// `*NOTIFY`.
    Notify,
    /// `*STATUS`.
    Status,
}

/// A message a CL command sent to its caller.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClMessage {
    /// The identifier (`CPF9801`), empty for an immediate message.
    pub id: String,
    /// The type.
    pub kind: MsgKind,
    /// The first-level text with its data substituted.
    pub text: String,
}

/// A spooled file's text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Spooled {
    /// The spooled file's name (`QPRTSPLF`).
    pub name: String,
    /// Its lines, page by page.
    pub pages: Vec<Vec<String>>,
}

/// A CL command for [`ClRunner`].
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ClRequest {
    /// The command string.
    pub command: String,
    /// Run in the shell's own job (`system -i`), not a new one.
    pub same_job: bool,
    /// Environment variables for the job (`system -e`); empty for none.
    pub env: Vec<(String, String)>,
    /// Keep the spooled files after returning their text (`-k`).
    pub keep_spool: bool,
    /// Keep the job log of a new job (`-K`).
    pub keep_joblog: bool,
}

/// What a CL command did.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ClOutcome {
    /// The messages it sent, in order.
    pub messages: Vec<ClMessage>,
    /// The spooled files it made.
    pub spooled: Vec<Spooled>,
    /// It ended with an escape message (or did not run).
    pub failed: bool,
}

/// Runs CL commands (`system`, `logger`, `Rfile -c`).
pub trait ClRunner: Send + Sync {
    /// Run one command.
    fn run_cl(&self, req: &ClRequest) -> Result<ClOutcome, SysError>;
}

/// An SQL statement for [`SqlRunner`].
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SqlRequest {
    /// The statement.
    pub statement: String,
    /// System naming (`LIB/FILE`) instead of SQL naming.
    pub system_naming: bool,
}

/// A result set column.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SqlColumn {
    /// The column's name or label.
    pub name: String,
    /// Its display width.
    pub width: usize,
    /// Numeric (right-aligned).
    pub numeric: bool,
}

/// What an SQL statement did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SqlOutcome {
    /// A result set; `None` is the null value.
    Rows {
        /// The columns.
        columns: Vec<SqlColumn>,
        /// The rows, each value as text.
        rows: Vec<Vec<Option<String>>>,
    },
    /// The statement ran; the rows it changed when it changes rows.
    Done {
        /// Rows inserted, updated or deleted.
        count: Option<u64>,
    },
    /// The statement failed.
    Error {
        /// The `SQLSTATE`.
        sqlstate: String,
        /// The `SQLCODE`.
        code: i32,
        /// The message text.
        text: String,
    },
}

/// Runs SQL statements (`db2`).
pub trait SqlRunner: Send + Sync {
    /// Run one statement.
    fn run_sql(&self, req: &SqlRequest) -> Result<SqlOutcome, SysError>;
}

/// Who the job runs as (`id`, `logname`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Credentials {
    /// The user profile's name.
    pub user: String,
    /// Its user ID.
    pub uid: u32,
    /// Its group profile's ID (0 for none).
    pub gid: u32,
    /// Supplemental group IDs.
    pub groups: Vec<u32>,
}

/// Which part of the library list a library is in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LibKind {
    /// The system portion.
    System,
    /// A product library.
    Product,
    /// The current library.
    Current,
    /// The user portion.
    User,
}

impl LibKind {
    /// The three letters `liblist` shows.
    pub fn code(self) -> &'static str {
        match self {
            Self::System => "SYS",
            Self::Product => "PRD",
            Self::Current => "CUR",
            Self::User => "USR",
        }
    }
}

/// One library list entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LibEntry {
    /// The library.
    pub name: String,
    /// Its portion.
    pub kind: LibKind,
}

/// A change to the job's library list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LiblChange {
    /// Add to the front of the user portion.
    AddFirst(String),
    /// Add to the end of the user portion.
    AddLast(String),
    /// Remove from the user portion.
    Remove(String),
    /// Make the current library (`*CRTDFT` for none).
    SetCurrent(String),
}

/// A data area's type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DtaaraType {
    /// `*CHAR`.
    Char,
    /// `*DEC` with its digits and decimal places.
    Dec(u32, u32),
    /// `*LGL`.
    Logical,
}

/// A data area's contents.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DataArea {
    /// The type.
    pub kind: DtaaraType,
    /// The length (characters or digits).
    pub length: usize,
    /// The value as text (a `*DEC` value with its sign and point).
    pub value: String,
}

/// A data queue operation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DtaqOp {
    /// Send an entry (with its key on a keyed queue).
    Send {
        /// The entry.
        data: Vec<u8>,
        /// The key.
        key: Option<Vec<u8>>,
    },
    /// Receive an entry: `wait` `None` waits forever; `remove` false
    /// leaves it on the queue.
    Receive {
        /// The key equal to which an entry is taken.
        key: Option<Vec<u8>>,
        /// How long to wait.
        wait: Option<Duration>,
        /// Take it off the queue.
        remove: bool,
    },
    /// Remove every entry.
    Clear,
    /// The number of entries.
    Count,
}

/// What a data queue operation returned.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DtaqReply {
    /// Done.
    Done,
    /// An entry, or `None` when the wait ended without one.
    Entry(Option<Vec<u8>>),
    /// The number of entries.
    Count(u64),
}

/// A spooled file of a job.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SplfInfo {
    /// The name.
    pub name: String,
    /// The number within the job.
    pub number: u32,
    /// Total pages.
    pub pages: u32,
    /// Status (`RDY`, `HLD`, ...).
    pub status: String,
}

/// Which spooled file of a name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SplfNumber {
    /// This number.
    Number(u32),
    /// The highest number.
    Last,
    /// The only one of the name.
    Only,
}

/// A System V IPC object's kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IpcKind {
    /// A message queue.
    Queue,
    /// A shared memory segment.
    Memory,
    /// A semaphore set.
    Semaphore,
}

/// One IPC object for `ipcs`. The kind-specific numbers are: `size`
/// the maximum bytes (queue), segment size or semaphores in the set;
/// `used` the bytes on the queue or attached processes; `count` the
/// messages on a queue; `pids` the last sender and receiver (queue) or
/// creator and last user (memory); `times` the last send, receive and
/// change (queue), attach, detach and change (memory) or operation, 0
/// and change (semaphore), seconds since the epoch, 0 for never.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IpcInfo {
    /// The kind.
    pub kind: IpcKind,
    /// The identifier.
    pub id: i32,
    /// The key (0 is `IPC_PRIVATE`).
    pub key: i32,
    /// Permission bits.
    pub mode: u32,
    /// Owner.
    pub uid: u32,
    /// Owner's group.
    pub gid: u32,
    /// Creator.
    pub cuid: u32,
    /// Creator's group.
    pub cgid: u32,
    /// See the type's description.
    pub size: u64,
    /// See the type's description.
    pub used: u64,
    /// See the type's description.
    pub count: u64,
    /// See the type's description.
    pub pids: (i32, i32),
    /// See the type's description.
    pub times: [i64; 3],
}

/// Which IPC object to remove.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IpcTarget {
    /// By identifier.
    Id(i32),
    /// By key.
    Key(i32),
}
