// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The dumps family: a partition's dumps, taken by the host from
//! outside (its processors stopped, its memory and processor state
//! written as an ELF core). List, take, delete, and download: the
//! download's reply opens a stream of the dump's bytes, host to
//! console, which ends after the last of them.

use ipc::wire_struct;

use super::model::{Note, PartitionId, check_id, check_name};
use crate::message::Empty;
use crate::{Family, family, messages};

/// One dump of a partition.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DumpInfo {
    /// Its name.
    pub name: String,
    /// When it was taken, seconds since the epoch.
    pub time: u64,
    /// Its size, bytes.
    pub bytes: u64,
    /// The bytes it takes in its pool (a page of zeroes takes none).
    pub allocated: u64,
    /// The partition's reference code when it was taken.
    pub code: String,
    /// Why it was taken, in the host's words (`function22`, `request`).
    pub reason: String,
}
wire_struct!(DumpInfo {
    name,
    time,
    bytes,
    allocated,
    code,
    reason
});

/// Ask for a partition's dumps.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct List {
    /// The partition.
    pub id: PartitionId,
}
wire_struct!(List { id });

/// Dump a running or held partition without restarting it. The answer
/// is the dump's name; it is written in the background and
/// [`Taken`] says when.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Take {
    /// The partition.
    pub id: PartitionId,
}
wire_struct!(Take { id });

/// The name a dump being taken has.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Begun {
    /// The dump's name.
    pub name: String,
}
wire_struct!(Begun { name });

/// Delete a dump.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Delete {
    /// The partition.
    pub id: PartitionId,
    /// The dump.
    pub name: String,
}
wire_struct!(Delete { id, name });

/// Download a dump: the reply opens a stream of its bytes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Download {
    /// The partition.
    pub id: PartitionId,
    /// The dump.
    pub name: String,
}
wire_struct!(Download { id, name });

/// A download's stream is open: this many bytes follow on it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Downloading {
    /// The dump.
    pub name: String,
    /// Its size, bytes.
    pub bytes: u64,
}
wire_struct!(Downloading { name, bytes });

/// A dump was written, or could not be.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Taken {
    /// The partition.
    pub id: PartitionId,
    /// Its name.
    pub name: String,
    /// The dump's name.
    pub dump: String,
    /// Why it was not written; `None` when it was.
    pub error: Option<Note>,
    /// When, seconds since the epoch.
    pub time: u64,
}
wire_struct!(Taken {
    id,
    name,
    dump,
    error,
    time
});

/// A dump was removed: deleted, or the oldest put out for the host's
/// limits.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Removed {
    /// The partition.
    pub id: PartitionId,
    /// The dump's name.
    pub dump: String,
    /// When, seconds since the epoch.
    pub time: u64,
}
wire_struct!(Removed { id, dump, time });

/// Operation numbers.
pub mod op {
    /// [`super::List`].
    pub const LIST: u16 = 0x0001;
    /// [`super::Take`].
    pub const TAKE: u16 = 0x0002;
    /// [`super::Delete`].
    pub const DELETE: u16 = 0x0003;
    /// [`super::Download`].
    pub const DOWNLOAD: u16 = 0x0004;
}

/// Event numbers.
pub mod event {
    /// [`super::Taken`].
    pub const TAKEN: u16 = 0x0001;
    /// [`super::Removed`].
    pub const REMOVED: u16 = 0x0002;
}

/// The family's own refusals (`QSI0550` to `QSI0599`): none in
/// version 1; a host refuses with its own.
pub mod msgid {}

messages! {
    /// What a console asks.
    pub enum Request: family::DUMPS => {
        /// A partition's dumps.
        0x0001 => List(List),
        /// Take one.
        0x0002 => Take(Take),
        /// Delete one.
        0x0003 => Delete(Delete),
        /// Download one.
        0x0004 => Download(Download),
    }
}

messages! {
    /// What the host answers.
    pub enum Reply: family::DUMPS => {
        /// The dumps, oldest first.
        0x0001 => List(Vec<DumpInfo>),
        /// Begun.
        0x0002 => Take(Begun),
        /// Deleted.
        0x0003 => Delete(Empty),
        /// The stream is open.
        0x0004 => Download(Downloading),
    }
}

messages! {
    /// What the host pushes.
    pub enum Event: family::DUMPS => {
        /// A dump was written, or could not be.
        0x0001 => Taken(Taken),
        /// A dump was removed.
        0x0002 => Removed(Removed),
    }
}

impl Request {
    /// The protocol's bounds.
    pub fn check(&self) -> Result<(), &'static str> {
        match self {
            Self::List(List { id }) | Self::Take(Take { id }) => check_id(*id),
            Self::Delete(Delete { id, name }) | Self::Download(Download { id, name }) => {
                check_id(*id)?;
                check_name(name)
            }
        }
    }
}

/// The dumps family.
#[derive(Debug)]
pub struct Dumps;

impl Family for Dumps {
    const ID: u16 = family::DUMPS;
    const VERSION: u16 = 1;
    const CAP: u64 = crate::caps::DUMPS;
    type Request = Request;
    type Reply = Reply;
    type Event = Event;
}
