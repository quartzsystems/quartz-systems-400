// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! One archive member as every format sees it.

use qsh::sys::{FileType, Stat};

/// What a member is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// A regular file.
    File,
    /// A directory.
    Dir,
    /// A symbolic link (`link` is the target).
    Symlink,
    /// Another name for an earlier member (`link` is its name).
    Hardlink,
    /// A FIFO.
    Fifo,
    /// A character special file.
    Char,
    /// A block special file.
    Block,
}

/// A member: header fields and data.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Member {
    /// The path name, without a final slash.
    pub name: String,
    /// What it is.
    pub kind: Kind,
    /// Permission bits with set-ID and sticky bits.
    pub mode: u32,
    /// Owner.
    pub uid: u32,
    /// Group.
    pub gid: u32,
    /// Owner's name, empty when unknown.
    pub uname: String,
    /// Group's name, empty when unknown.
    pub gname: String,
    /// Modification time.
    pub mtime: i64,
    /// Link target.
    pub link: String,
    /// The contents (a hard link may carry a copy for cpio).
    pub data: Vec<u8>,
    /// Device of the original file.
    pub dev: u64,
    /// File ID of the original file.
    pub ino: u64,
    /// Link count.
    pub nlink: u32,
    /// Device numbers of a special file.
    pub rdev: (u32, u32),
}

impl Member {
    /// An empty member of a kind.
    pub fn new(name: &str, kind: Kind) -> Self {
        Self {
            name: name.to_string(),
            kind,
            mode: 0o644,
            uid: 0,
            gid: 0,
            uname: String::new(),
            gname: String::new(),
            mtime: 0,
            link: String::new(),
            data: Vec::new(),
            dev: 0,
            ino: 0,
            nlink: 1,
            rdev: (0, 0),
        }
    }

    /// The size shown in listings.
    pub fn size(&self) -> u64 {
        match self.kind {
            Kind::File => self.data.len() as u64,
            Kind::Symlink => self.link.len() as u64,
            _ => 0,
        }
    }

    /// A status for `ls`-style mode strings.
    pub fn stat(&self) -> Stat {
        let ftype = match self.kind {
            Kind::File | Kind::Hardlink => FileType::File,
            Kind::Dir => FileType::Dir,
            Kind::Symlink => FileType::Symlink,
            Kind::Fifo => FileType::Fifo,
            Kind::Char => FileType::Char,
            Kind::Block => FileType::Block,
        };
        Stat {
            ftype,
            mode: self.mode,
            size: self.size(),
            uid: self.uid,
            gid: self.gid,
            mtime: self.mtime,
            ino: self.ino,
            dev: self.dev,
            nlink: self.nlink,
            blocks: 0,
        }
    }
}

/// The `st_mode` type bits of a kind (cpio).
pub fn type_bits(k: Kind) -> u32 {
    match k {
        Kind::File | Kind::Hardlink => 0o100_000,
        Kind::Dir => 0o040_000,
        Kind::Symlink => 0o120_000,
        Kind::Fifo => 0o010_000,
        Kind::Char => 0o020_000,
        Kind::Block => 0o060_000,
    }
}

/// The kind of `st_mode` type bits.
pub fn kind_of_bits(m: u32) -> Option<Kind> {
    Some(match m & 0o170_000 {
        0o100_000 | 0 => Kind::File,
        0o040_000 => Kind::Dir,
        0o120_000 => Kind::Symlink,
        0o010_000 => Kind::Fifo,
        0o020_000 => Kind::Char,
        0o060_000 => Kind::Block,
        _ => return None,
    })
}
