// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `Ifs` (version 5; `Stat` version 6): integrated file system files. Paths are absolute
//! and UTF-8. Flags are `bool`s: one byte, 0 or 1, any other value does
//! not decode.

use ipc::{wire_enum, wire_struct};

/// What an entry is.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum EntryKind {
    /// A stream file.
    #[default]
    File,
    /// A directory.
    Directory,
    /// A symbolic link.
    Symlink,
    /// Anything else.
    Other,
}

wire_enum!(EntryKind {
    0 => File,
    1 => Directory,
    2 => Symlink,
    3 => Other,
});

impl EntryKind {
    /// Every kind, in value order.
    pub const ALL: [EntryKind; 4] = [
        EntryKind::File,
        EntryKind::Directory,
        EntryKind::Symlink,
        EntryKind::Other,
    ];
}

/// A file, directory or link as lists show it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Entry {
    /// Its name, the last component of its path.
    pub name: String,
    /// What it is.
    pub kind: EntryKind,
    /// Its size, bytes.
    pub size: u64,
    /// Last changed, milliseconds since 1970-01-01 UTC.
    pub modified: i64,
    /// Its owner.
    pub owner: String,
    /// Its CCSID.
    pub ccsid: u16,
    /// A symbolic link's target.
    pub target: Option<String>,
}

wire_struct!(Entry {
    name,
    kind,
    size,
    modified,
    owner,
    ccsid,
    target
});

/// How `WriteFile` writes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum WriteMode {
    /// The file must not exist.
    #[default]
    Create,
    /// Created, or emptied first.
    Replace,
    /// At `offset` in an existing file.
    Update,
    /// At the end.
    Append,
}

wire_enum!(WriteMode {
    0 => Create,
    1 => Replace,
    2 => Update,
    3 => Append,
});

impl WriteMode {
    /// Every mode, in value order.
    pub const ALL: [WriteMode; 4] = [
        WriteMode::Create,
        WriteMode::Replace,
        WriteMode::Update,
        WriteMode::Append,
    ];
}

/// `ListDirectory`'s request.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ListDirectory {
    /// The directory.
    pub path: String,
    /// Names to list: `*` and `?` wildcards.
    pub pattern: String,
    /// The first entry, 0-based.
    pub start: u32,
    /// At most this many; 0 for no limit.
    pub max: u32,
}

wire_struct!(ListDirectory {
    path,
    pattern,
    start,
    max
});

/// `ListDirectory`'s reply.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Entries {
    /// Entries matching.
    pub total: u32,
    /// Those from `start`.
    pub entries: Vec<Entry>,
}

wire_struct!(Entries { total, entries });

/// The request of `GetInfo` and `Delete`: one path.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PathRef {
    /// The path.
    pub path: String,
}

wire_struct!(PathRef { path });

/// `ReadFile`'s request.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReadFile {
    /// The file.
    pub path: String,
    /// The first byte.
    pub offset: u64,
    /// Bytes of the file to read; 0 for to the end.
    pub length: u64,
    /// Convert from the file's CCSID to this one.
    pub as_ccsid: Option<u16>,
}

wire_struct!(ReadFile {
    path,
    offset,
    length,
    as_ccsid
});

/// `ReadFile`'s reply.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FileData {
    /// The file's size now.
    pub size: u64,
    /// The file offset just past the last byte read or converted.
    pub next_offset: u64,
    /// The data.
    pub data: Vec<u8>,
}

wire_struct!(FileData {
    size,
    next_offset,
    data
});

/// `WriteFile`'s request.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WriteFile {
    /// The file.
    pub path: String,
    /// How to write.
    pub mode: WriteMode,
    /// Where, for `Update`.
    pub offset: u64,
    /// The CCSID of a file created; 0 for 1208.
    pub ccsid: u16,
    /// The data.
    pub data: Vec<u8>,
    /// The CCSID `data` is in, converted to the file's.
    pub data_ccsid: Option<u16>,
}

wire_struct!(WriteFile {
    path,
    mode,
    offset,
    ccsid,
    data,
    data_ccsid
});

/// `WriteFile`'s reply.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Written {
    /// The file's size in its own CCSID.
    pub size: u64,
}

wire_struct!(Written { size });

/// `CreateDirectory`'s request.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CreateDirectory {
    /// The directory.
    pub path: String,
    /// Create the missing directories above it too.
    pub parents: bool,
}

wire_struct!(CreateDirectory { path, parents });

/// The request of `Rename` and `Copy`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FromTo {
    /// The object.
    pub from: String,
    /// Its new path.
    pub to: String,
    /// Replace `to` when it exists.
    pub replace: bool,
}

wire_struct!(FromTo { from, to, replace });

/// `Stat`'s reply (version 6).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Stat {
    /// What `GetInfo` answers.
    pub entry: Entry,
    /// The object's change counter: up on every change, never reused.
    pub change: u64,
}

wire_struct!(Stat { entry, change });
