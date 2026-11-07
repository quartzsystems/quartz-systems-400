// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The integrated file system: browse, properties, edit, transfers,
//! shares (handoff steps 13-14). Every call names the signed-on user
//! and runs with that user's authority; a refusal is the system's
//! message (`CPFA0xx`, `CPE3xxx`...).

use qsweb::msg::Message;

use crate::transfer::Queue;

/// What an entry is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Kind {
    /// A directory.
    Dir,
    /// A stream file.
    File,
    /// A symbolic link: its target, whether that is a directory, and
    /// whether it resolves at all.
    Link {
        /// The target as stored.
        target: String,
        /// The target is a directory.
        dir: bool,
        /// The target does not exist.
        broken: bool,
    },
    /// Anything else (`*LIB`, `*FILE`, `*MBR`, `*SAVF`, ...), by its
    /// object type.
    Other(String),
}

impl Kind {
    /// Whether opening it lists entries.
    pub fn is_dir(&self) -> bool {
        match self {
            Kind::Dir => true,
            Kind::Link { dir, broken, .. } => *dir && !*broken,
            Kind::File | Kind::Other(_) => false,
        }
    }
}

/// One entry of a directory.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    /// The name in its directory.
    pub name: String,
    /// What it is.
    pub kind: Kind,
    /// Bytes (0 for a directory).
    pub size: u64,
    /// Last change of the data, seconds since 1970.
    pub modified: u64,
    /// The owner.
    pub owner: String,
    /// The permission bits (`0o640`).
    pub mode: u32,
    /// The data's CCSID, 0 when it has none (directories, links).
    pub ccsid: u16,
}

/// What the Properties page shows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Props {
    /// The entry.
    pub entry: Entry,
    /// The object type as the page names it (`Stream file (*STMF)`).
    pub typ: String,
    /// Created, seconds since 1970.
    pub created: u64,
    /// Last used (read), seconds since 1970.
    pub accessed: u64,
    /// Change count.
    pub changes: u64,
    /// The primary group, `*NONE` for none.
    pub group: String,
    /// The authorization list, `*NONE` for none.
    pub autl: String,
}

/// A change to an entry's authority (`CHGAUT`, `chmod`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuthChange {
    /// The permission bits.
    pub mode: u32,
    /// The authorization list, `*NONE` for none.
    pub autl: String,
}

/// A home directory or a share, as My files lists it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Share {
    /// `Home`, the share's text.
    pub name: String,
    /// Its path.
    pub path: String,
    /// `Directory`, `Share (SMB: DOCS)`, `Library`.
    pub typ: String,
    /// Bytes used under it, `None` when not known.
    pub used: Option<u64>,
}

/// The integrated file system as the signed-on user sees it.
pub trait Files: Send + Sync {
    /// The user's home directory.
    fn home(&self, user: &str) -> String;
    /// The entries of directory `dir`, `.` and `..` left out.
    fn list(&self, user: &str, dir: &str) -> Result<Vec<Entry>, Message>;
    /// One entry, a link not followed.
    fn entry(&self, user: &str, path: &str) -> Result<Entry, Message>;
    /// Everything the Properties page shows.
    fn props(&self, user: &str, path: &str) -> Result<Props, Message>;
    /// `len` bytes of `path` from `offset` (fewer at its end).
    fn read(&self, user: &str, path: &str, offset: u64, len: u64) -> Result<Vec<u8>, Message>;
    /// Write `data` as all of `path`, made with `ccsid` when it is new;
    /// an existing file keeps its owner and authority.
    fn write(&self, user: &str, path: &str, data: &[u8], ccsid: u16) -> Result<(), Message>;
    /// Make directory `path`.
    fn mkdir(&self, user: &str, path: &str) -> Result<(), Message>;
    /// Rename or move `from` to `to`.
    fn rename(&self, user: &str, from: &str, to: &str) -> Result<(), Message>;
    /// Copy `from` (a directory with what it holds) to `to`, on the
    /// system: the data does not pass through Access for Web.
    fn copy(&self, user: &str, from: &str, to: &str) -> Result<(), Message>;
    /// Delete `path`, a directory with everything in it; the entries
    /// deleted, or where it stopped (the entry and the system's
    /// message) with the count deleted before it.
    fn delete(&self, user: &str, path: &str) -> Result<u64, (String, Message, u64)>;
    /// The entries a delete of `path` would remove, itself included.
    fn count(&self, user: &str, path: &str) -> Result<u64, Message>;
    /// Change the authority of `path`.
    fn set_auth(&self, user: &str, path: &str, change: &AuthChange) -> Result<(), Message>;
    /// The home directory and the shares the user may use.
    fn shares(&self, user: &str) -> Vec<Share>;
    /// The transfer queues of the users signed on.
    fn queue(&self) -> &Queue;
}
