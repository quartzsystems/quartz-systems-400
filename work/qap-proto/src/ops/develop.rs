// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `Develop` (version 8, draft): the library list, libraries, objects,
//! source members and compiles, for Code for QS/400. Each operation
//! does what its command does (docs/QAP.md, "Service 9 `Develop`").

use ipc::{wire_enum, wire_struct};

use crate::envelope::ServerMessage;
use crate::types::{Diagnostic, JobId, MemberName, ObjectName, SourceRecord};

/// The connection job's library list (`GetLibraryList`'s reply, and
/// `SetLibraryList`'s).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LibraryList {
    /// The current library; `None` when there is none.
    pub current: Option<String>,
    /// The system part, then the product libraries, in search order.
    pub system: Vec<String>,
    /// The user part, in search order.
    pub user: Vec<String>,
}

wire_struct!(LibraryList {
    current,
    system,
    user
});

/// `SetLibraryList`'s request: `CHGLIBL`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SetLibraryList {
    /// `CURLIB`; `None` for `*CRTDFT`, no current library.
    pub current: Option<String>,
    /// `LIBL`; empty for `*NONE`.
    pub user: Vec<String>,
}

wire_struct!(SetLibraryList { current, user });

/// A library's `TYPE`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum LibraryKind {
    /// `*PROD`.
    #[default]
    Production,
    /// `*TEST`.
    Test,
}

wire_enum!(LibraryKind {
    0 => Production,
    1 => Test,
});

/// `ListLibraries`'s request.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ListLibraries {
    /// A generic name; empty for `*ALL`.
    pub pattern: String,
    /// The first entry, 0-based.
    pub start: u32,
    /// At most this many; 0 for no limit.
    pub max: u32,
}

wire_struct!(ListLibraries {
    pattern,
    start,
    max
});

/// A library.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Library {
    /// Its name.
    pub name: String,
    /// Its type.
    pub kind: LibraryKind,
    /// Its text.
    pub text: String,
}

wire_struct!(Library { name, kind, text });

/// `ListLibraries`'s reply.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Libraries {
    /// Libraries matching.
    pub total: u32,
    /// Those returned, by name.
    pub libraries: Vec<Library>,
}

wire_struct!(Libraries { total, libraries });

/// `ListObjects`'s request.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ListObjects {
    /// `*LIBL`, `*CURLIB`, `*USRLIBL`, `*ALL` or a library.
    pub library: String,
    /// A generic name; empty for `*ALL`.
    pub pattern: String,
    /// The types wanted (`*FILE`); empty for every type.
    pub types: Vec<String>,
    /// The attribute wanted (`RPGLE`); `None` for any.
    pub attribute: Option<String>,
    /// The first entry, 0-based.
    pub start: u32,
    /// At most this many; 0 for no limit.
    pub max: u32,
}

wire_struct!(ListObjects {
    library,
    pattern,
    types,
    attribute,
    start,
    max
});

/// An object, as `WRKOBJ` lists it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ObjectEntry {
    /// Its library.
    pub library: String,
    /// Its name.
    pub name: String,
    /// Its type (`*FILE`).
    pub r#type: String,
    /// Its attribute (`PF`, `RPGLE`); empty when none.
    pub attribute: String,
    /// Its text.
    pub text: String,
    /// Its size in bytes.
    pub size: u64,
    /// When it last changed, milliseconds since 1970-01-01 UTC.
    pub changed: i64,
    /// Its owner.
    pub owner: String,
}

wire_struct!(ObjectEntry {
    library,
    name,
    r#type,
    attribute,
    text,
    size,
    changed,
    owner
});

/// `ListObjects`'s reply.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Objects {
    /// Objects matching.
    pub total: u32,
    /// Those returned: by library in search order, then name, then type.
    pub objects: Vec<ObjectEntry>,
}

wire_struct!(Objects { total, objects });

/// `DescribeObject`'s request.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DescribeObject {
    /// The object; its library may be `*LIBL` or `*CURLIB`.
    pub object: ObjectName,
    /// Its type (`*PGM`).
    pub r#type: String,
}

wire_struct!(DescribeObject { object, r#type });

/// One line of an object's description.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Attribute {
    /// The server's label (`Owner`).
    pub name: String,
    /// The value, as `DSPOBJD` shows it.
    pub value: String,
}

wire_struct!(Attribute { name, value });

/// `DescribeObject`'s reply.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ObjectDescription {
    /// The attributes, in `DSPOBJD DETAIL(*FULL)` order.
    pub attributes: Vec<Attribute>,
}

wire_struct!(ObjectDescription { attributes });

/// `ListMembers`'s request.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ListMembers {
    /// The file; its library may be `*LIBL` or `*CURLIB`.
    pub file: ObjectName,
    /// A generic name; empty for `*ALL`.
    pub pattern: String,
    /// The first entry, 0-based.
    pub start: u32,
    /// At most this many; 0 for no limit.
    pub max: u32,
}

wire_struct!(ListMembers {
    file,
    pattern,
    start,
    max
});

/// A member, as `WRKMBRPDM` lists it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MemberEntry {
    /// Its name.
    pub name: String,
    /// Its source type; empty when none.
    pub srctype: String,
    /// Its text.
    pub text: String,
    /// Its records.
    pub records: u32,
    /// When it was added, milliseconds since 1970-01-01 UTC.
    pub created: i64,
    /// When it last changed.
    pub changed: i64,
}

wire_struct!(MemberEntry {
    name,
    srctype,
    text,
    records,
    created,
    changed
});

/// `ListMembers`'s reply.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Members {
    /// Members matching.
    pub total: u32,
    /// Those returned, by name.
    pub members: Vec<MemberEntry>,
}

wire_struct!(Members { total, members });

/// `ReadMember`'s request.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReadMember {
    /// The member.
    pub member: MemberName,
    /// Hold it `*SHRNUP` for the connection, as `STRSEU` does.
    pub lock: bool,
}

wire_struct!(ReadMember { member, lock });

/// `ReadMember`'s reply.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MemberData {
    /// Its source type.
    pub srctype: String,
    /// Its text.
    pub text: String,
    /// A record's data length, in the file's bytes.
    pub width: u16,
    /// The file's CCSID.
    pub ccsid: u16,
    /// The records, in order.
    pub records: Vec<SourceRecord>,
}

wire_struct!(MemberData {
    srctype,
    text,
    width,
    ccsid,
    records
});

/// `WriteMember`'s request: one piece of the member's new records.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WriteMember {
    /// The member.
    pub member: MemberName,
    /// The piece's records.
    pub records: Vec<SourceRecord>,
    /// Number the records from `0001.00` in `0001.00` steps.
    pub renumber: bool,
    /// The new source type; `None` leaves it.
    pub srctype: Option<String>,
    /// The new text; `None` leaves it.
    pub text: Option<String>,
    /// The first piece.
    pub first: bool,
    /// The last piece: the member is written.
    pub last: bool,
}

wire_struct!(WriteMember {
    member,
    records,
    renumber,
    srctype,
    text,
    first,
    last
});

/// `WriteMember`'s reply.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Written {
    /// Records received so far, or the member's records once written.
    pub records: u32,
}

wire_struct!(Written { records });

/// A member alone: `UnlockMember`'s and `DeleteMember`'s request.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MemberRef {
    /// The member.
    pub member: MemberName,
}

wire_struct!(MemberRef { member });

/// `CreateMember`'s request: `ADDPFM`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CreateMember {
    /// The member to add.
    pub member: MemberName,
    /// Its source type.
    pub srctype: String,
    /// Its text.
    pub text: String,
}

wire_struct!(CreateMember {
    member,
    srctype,
    text
});

/// `CopyMember`'s request: `CPYSRCF`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CopyMember {
    /// The member copied.
    pub from: MemberName,
    /// The copy.
    pub to: MemberName,
    /// Replace `to` when it exists.
    pub replace: bool,
}

wire_struct!(CopyMember { from, to, replace });

/// `RenameMember`'s request: `RNMM`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RenameMember {
    /// The member.
    pub member: MemberName,
    /// Its new name.
    pub new_name: String,
}

wire_struct!(RenameMember { member, new_name });

/// `ChangeMember`'s request: `CHGPFM`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ChangeMember {
    /// The member.
    pub member: MemberName,
    /// The new source type; `None` leaves it.
    pub srctype: Option<String>,
    /// The new text; `None` leaves it.
    pub text: Option<String>,
}

wire_struct!(ChangeMember {
    member,
    srctype,
    text
});

/// `Compile`'s reply: `RunCommand`'s, and the compiler's diagnostics.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CompileReply {
    /// The job the command ran in.
    pub job: JobId,
    /// Every message the command sent its caller, oldest first.
    pub messages: Vec<ServerMessage>,
    /// What the compiler reported, as its listing shows it.
    pub diagnostics: Vec<Diagnostic>,
}

wire_struct!(CompileReply {
    job,
    messages,
    diagnostics
});
