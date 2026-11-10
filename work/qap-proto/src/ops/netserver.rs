// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `NetServer` (version 9, draft): file and print shares, sessions,
//! the server's attributes, users disabled for NetServer, start and
//! end, each as the `QZLS*` API a program would call does it
//! (docs/QAP.md, "Service 12 `NetServer`").

#![allow(clippy::doc_markdown)] // NetServer

use ipc::{wire_enum, wire_struct};

use crate::types::{JobId, ObjectName};

/// A share's kind.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ShareKind {
    /// A directory.
    #[default]
    File,
    /// An output queue.
    Print,
}

wire_enum!(ShareKind {
    0 => File,
    1 => Print,
});

/// How a print share makes spooled files.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SpoolType {
    /// `*AUTO`: sensed from the data.
    #[default]
    Auto,
    /// `*USERASCII`.
    UserAscii,
    /// `*AFP`.
    Afp,
    /// `*SCS`.
    Scs,
}

wire_enum!(SpoolType {
    0 => Auto,
    1 => UserAscii,
    2 => Afp,
    3 => Scs,
});

impl SpoolType {
    /// Every type, in value order.
    pub const ALL: [SpoolType; 4] = [
        SpoolType::Auto,
        SpoolType::UserAscii,
        SpoolType::Afp,
        SpoolType::Scs,
    ];
}

/// What a file share shares.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FileShare {
    /// The directory, from `/`.
    pub path: String,
    /// Read only (else read/write).
    pub read_only: bool,
    /// Connections at once; `None` for no maximum.
    pub max_users: Option<u32>,
    /// Whether files with the extensions are converted as text.
    pub text_conversion: bool,
    /// The CCSID clients see converted text in; 0 the client's.
    pub text_ccsid: u32,
    /// The extensions converted, upper case, without the dot.
    pub extensions: Vec<String>,
}

wire_struct!(FileShare {
    path,
    read_only,
    max_users,
    text_conversion,
    text_ccsid,
    extensions
});

/// What a print share shares.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PrintShare {
    /// The output queue.
    pub output_queue: ObjectName,
    /// The printer driver clients are told to use.
    pub driver: String,
    /// How data becomes a spooled file.
    pub spool_type: SpoolType,
    /// The printer file the spooled files take their attributes from;
    /// `None` for the default.
    pub printer_file: Option<ObjectName>,
}

wire_struct!(PrintShare {
    output_queue,
    driver,
    spool_type,
    printer_file
});

/// A share's own part.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShareDetail {
    /// A file share.
    File {
        /// What it shares.
        file: FileShare,
    },
    /// A print share.
    Print {
        /// What it shares.
        print: PrintShare,
    },
}

impl Default for ShareDetail {
    fn default() -> Self {
        Self::File {
            file: FileShare::default(),
        }
    }
}

wire_enum!(ShareDetail {
    0 => File { file },
    1 => Print { print },
});

/// A share.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Share {
    /// Up to 12 characters, upper case.
    pub name: String,
    /// Up to 50 characters.
    pub text: String,
    /// Only clients that encrypt may use it.
    pub encryption_required: bool,
    /// Only users with `*USE` to this list may use it.
    pub authorization_list: Option<String>,
    /// Connections to it now (ignored in requests).
    pub current_users: u32,
    /// File or print.
    pub detail: ShareDetail,
}

wire_struct!(Share {
    name,
    text,
    encryption_required,
    authorization_list,
    current_users,
    detail
});

/// `ListShares`' request.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ListShares {
    /// Only shares of this kind; `None` for both.
    pub kind: Option<ShareKind>,
    /// The first entry, 0-based.
    pub start: u32,
    /// At most this many; 0 for no limit.
    pub max: u32,
}

wire_struct!(ListShares { kind, start, max });

/// `ListShares`' reply.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Shares {
    /// Shares matching.
    pub total: u32,
    /// Those returned, by name.
    pub shares: Vec<Share>,
}

wire_struct!(Shares { total, shares });

/// A share by name: `GetShare`'s and `RemoveShare`'s request.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ShareRef {
    /// The share.
    pub name: String,
}

wire_struct!(ShareRef { name });

/// A share whole: `AddShare`'s and `ChangeShare`'s request.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ShareBody {
    /// The share.
    pub share: Share,
}

wire_struct!(ShareBody { share });

/// A connection's kind.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ConnectionKind {
    /// A file share.
    #[default]
    Disk,
    /// A print share.
    Print,
    /// The interprocess share (`IPC$`).
    Ipc,
}

wire_enum!(ConnectionKind {
    0 => Disk,
    1 => Print,
    2 => Ipc,
});

/// A session's connection to a share.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Connection {
    /// The share.
    pub share: String,
    /// Its kind.
    pub kind: ConnectionKind,
    /// Files open through it.
    pub open_files: u32,
    /// Since, milliseconds since 1970-01-01 UTC.
    pub since: i64,
}

wire_struct!(Connection {
    share,
    kind,
    open_files,
    since
});

/// A session: one user signed on from one client.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NetSession {
    /// Its identifier (`QZLSENSS`'s).
    pub id: u64,
    /// The client's name (up to 15 characters) or address.
    pub workstation: String,
    /// The client's full name or address.
    pub expanded: String,
    /// The profile signed on.
    pub user: String,
    /// Signed on as the guest profile.
    pub guest: bool,
    /// The job serving it.
    pub job: Option<JobId>,
    /// Since, milliseconds since 1970-01-01 UTC.
    pub since: i64,
    /// Its last request, milliseconds since 1970-01-01 UTC.
    pub last_active: i64,
    /// Its connections.
    pub connections: Vec<Connection>,
}

wire_struct!(NetSession {
    id,
    workstation,
    expanded,
    user,
    guest,
    job,
    since,
    last_active,
    connections
});

/// `ListSessions`' request.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ListSessions {
    /// The first entry, 0-based.
    pub start: u32,
    /// At most this many; 0 for no limit.
    pub max: u32,
}

wire_struct!(ListSessions { start, max });

/// `ListSessions`' reply.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NetSessions {
    /// Sessions.
    pub total: u32,
    /// Those returned, by identifier.
    pub sessions: Vec<NetSession>,
}

wire_struct!(NetSessions { total, sessions });

/// A session by identifier: `EndSession`'s request.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SessionRef {
    /// The session.
    pub id: u64,
}

wire_struct!(SessionRef { id });

/// The SMB2 dialects, oldest first.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Dialect {
    /// 2.0.2.
    #[default]
    Smb202,
    /// 2.1.
    Smb21,
    /// 3.0.
    Smb30,
    /// 3.0.2.
    Smb302,
    /// 3.1.1.
    Smb311,
}

wire_enum!(Dialect {
    0 => Smb202,
    1 => Smb21,
    2 => Smb30,
    3 => Smb302,
    4 => Smb311,
});

impl Dialect {
    /// Every dialect, in value order.
    pub const ALL: [Dialect; 5] = [
        Dialect::Smb202,
        Dialect::Smb21,
        Dialect::Smb30,
        Dialect::Smb302,
        Dialect::Smb311,
    ];
}

/// The server's attributes, one side (current or pending).
#[allow(clippy::struct_excessive_bools)] // the attributes are flags
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ServerAttributes {
    /// The server name, up to 15 characters.
    pub server_name: String,
    /// The domain (workgroup), up to 15 characters.
    pub domain: String,
    /// The text, up to 50 characters.
    pub text: String,
    /// The server answers to the system name too.
    pub allow_system_name: bool,
    /// The guest profile; `None` for no guest access.
    pub guest: Option<String>,
    /// The code page of NetBIOS names; 0 the job's.
    pub code_page: i32,
    /// Seconds before an idle session ends; -1 never.
    pub idle_timeout: i32,
    /// Seconds a lease break waits; -1 no leases.
    pub oplock_timeout: i32,
    /// Milliseconds between browse announcements.
    pub browse_interval: i32,
    /// Message authentication: 0 and 1 signing offered, 2 required.
    pub message_auth: u8,
    /// Every connection must be encrypted.
    pub encryption_required: bool,
    /// Users need `*USE` to this list to connect; `None` for none.
    pub authorization_list: Option<String>,
    /// Also on port 139 and NetBIOS name service.
    pub allow_netbios: bool,
    /// The oldest dialect a client may negotiate.
    pub min_dialect: Dialect,
    /// A message to `QSYSOPR` at each sign-on.
    pub log_signon: bool,
}

wire_struct!(ServerAttributes {
    server_name,
    domain,
    text,
    allow_system_name,
    guest,
    code_page,
    idle_timeout,
    oplock_timeout,
    browse_interval,
    message_auth,
    encryption_required,
    authorization_list,
    allow_netbios,
    min_dialect,
    log_signon
});

/// `GetAttributes`' reply.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Attributes {
    /// `QZLSSERVER` is active.
    pub active: bool,
    /// Its job, while active.
    pub job: Option<JobId>,
    /// In effect.
    pub current: ServerAttributes,
    /// Taken at a start with reset or the next IPL.
    pub pending: ServerAttributes,
}

wire_struct!(Attributes {
    active,
    job,
    current,
    pending
});

/// `ChangeAttributes`' request: the pending attributes wanted. Its
/// reply is the pending attributes as they now are.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ChangeAttributes {
    /// The pending side wanted.
    pub pending: ServerAttributes,
}

wire_struct!(ChangeAttributes { pending });

/// A user disabled for NetServer.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DisabledUser {
    /// The profile.
    pub user: String,
    /// Since, milliseconds since 1970-01-01 UTC.
    pub since: i64,
}

wire_struct!(DisabledUser { user, since });

/// `ListDisabledUsers`' reply.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DisabledUsers {
    /// By name.
    pub users: Vec<DisabledUser>,
}

wire_struct!(DisabledUsers { users });

/// `EnableUser`'s request.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EnableUser {
    /// The profile.
    pub user: String,
}

wire_struct!(EnableUser { user });

/// `StartServer`'s request.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StartServer {
    /// Make the pending attributes current first.
    pub reset: bool,
}

wire_struct!(StartServer { reset });
