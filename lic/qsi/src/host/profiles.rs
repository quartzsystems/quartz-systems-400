// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The profiles family: a partition's profiles. List, get, put,
//! delete, set the default, and save what a partition holds now as a
//! profile. Each profile has a revision, a number the managed side
//! gives it and changes whenever the profile does: a put carries the
//! revision it was read at and is refused with [`msgid::CHANGED`] when
//! the profile changed meanwhile.

use ipc::wire_struct;

use super::model::{PartitionId, Profile, check_id, check_name};
use crate::message::Empty;
use crate::{Family, family, messages};

/// Ask for a partition's profiles.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct List {
    /// The partition.
    pub id: PartitionId,
}
wire_struct!(List { id });

/// A profile, as a list shows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    /// Its name.
    pub name: String,
    /// Its revision.
    pub revision: u64,
    /// Whether it is the partition's default.
    pub default: bool,
}
wire_struct!(Entry {
    name,
    revision,
    default
});

/// Ask for one profile.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Get {
    /// The partition.
    pub id: PartitionId,
    /// The profile.
    pub name: String,
}
wire_struct!(Get { id, name });

/// A profile and its revision.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Revised {
    /// The profile.
    pub profile: Profile,
    /// Its revision.
    pub revision: u64,
}
wire_struct!(Revised { profile, revision });

/// Create a profile or replace the one of its name. The first profile
/// of a partition becomes its default. A partition may be running: its
/// profile is the next activation's.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Put {
    /// The partition.
    pub id: PartitionId,
    /// The profile.
    pub profile: Profile,
    /// The revision it was read at; `None` for a new profile, refused
    /// with [`msgid::EXISTS`] when one of its name is there.
    pub revision: Option<u64>,
}
wire_struct!(Put {
    id,
    profile,
    revision
});

/// A put's or a save's answer: the profile's revision now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Saved {
    /// The revision.
    pub revision: u64,
}
wire_struct!(Saved { revision });

/// Delete a profile; not the default one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Delete {
    /// The partition.
    pub id: PartitionId,
    /// The profile.
    pub name: String,
}
wire_struct!(Delete { id, name });

/// Make a profile the partition's default.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SetDefault {
    /// The partition.
    pub id: PartitionId,
    /// The profile.
    pub name: String,
}
wire_struct!(SetDefault { id, name });

/// Save an activated partition's configuration as it is now (the
/// profile it runs with, at the resources it holds) as a profile.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SaveCurrent {
    /// The partition.
    pub id: PartitionId,
    /// The profile's name: a new one, or one to replace.
    pub name: String,
    /// When `name` is there: the revision it was read at, refused with
    /// [`msgid::CHANGED`] when it changed; `None` refuses a name that
    /// is there.
    pub revision: Option<u64>,
}
wire_struct!(SaveCurrent { id, name, revision });

/// A profile was created, changed or deleted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Changed {
    /// The partition.
    pub id: PartitionId,
    /// The profile.
    pub name: String,
    /// Its revision now; `None` when it was deleted.
    pub revision: Option<u64>,
    /// Whether it is the partition's default now.
    pub default: bool,
    /// When, seconds since the epoch.
    pub time: u64,
}
wire_struct!(Changed {
    id,
    name,
    revision,
    default,
    time
});

/// Operation numbers.
pub mod op {
    /// [`super::List`].
    pub const LIST: u16 = 0x0001;
    /// [`super::Get`].
    pub const GET: u16 = 0x0002;
    /// [`super::Put`].
    pub const PUT: u16 = 0x0003;
    /// [`super::Delete`].
    pub const DELETE: u16 = 0x0004;
    /// [`super::SetDefault`].
    pub const SET_DEFAULT: u16 = 0x0005;
    /// [`super::SaveCurrent`].
    pub const SAVE_CURRENT: u16 = 0x0006;
}

/// Event numbers.
pub mod event {
    /// [`super::Changed`].
    pub const CHANGED: u16 = 0x0001;
}

/// The family's own refusals (`QSI0401` to `QSI0449`).
pub mod msgid {
    /// The profile changed since the revision the request carries.
    pub const CHANGED: &str = "QSI0401";
    /// A profile of that name is there and the request carries no
    /// revision.
    pub const EXISTS: &str = "QSI0402";
    /// Save current of a partition that is not activated: it holds
    /// nothing to save.
    pub const NOT_ACTIVE: &str = "QSI0403";
}

messages! {
    /// What a console asks.
    pub enum Request: family::PROFILES => {
        /// A partition's profiles.
        0x0001 => List(List),
        /// One profile.
        0x0002 => Get(Get),
        /// Create or replace one.
        0x0003 => Put(Put),
        /// Delete one.
        0x0004 => Delete(Delete),
        /// Set the default.
        0x0005 => SetDefault(SetDefault),
        /// Save the current configuration.
        0x0006 => SaveCurrent(SaveCurrent),
    }
}

messages! {
    /// What the host answers.
    pub enum Reply: family::PROFILES => {
        /// The profiles, by name.
        0x0001 => List(Vec<Entry>),
        /// The profile and its revision.
        0x0002 => Get(Revised),
        /// Its revision now.
        0x0003 => Put(Saved),
        /// Deleted.
        0x0004 => Delete(Empty),
        /// Set.
        0x0005 => SetDefault(Empty),
        /// The saved profile's revision.
        0x0006 => SaveCurrent(Saved),
    }
}

messages! {
    /// What the host pushes.
    pub enum Event: family::PROFILES => {
        /// A profile was created, changed or deleted.
        0x0001 => Changed(Changed),
    }
}

impl Request {
    /// The protocol's bounds.
    pub fn check(&self) -> Result<(), &'static str> {
        match self {
            Self::List(l) => check_id(l.id),
            Self::Get(Get { id, name })
            | Self::Delete(Delete { id, name })
            | Self::SetDefault(SetDefault { id, name })
            | Self::SaveCurrent(SaveCurrent { id, name, .. }) => {
                check_id(*id)?;
                check_name(name)
            }
            Self::Put(p) => {
                check_id(p.id)?;
                p.profile.check()
            }
        }
    }
}

/// The profiles family.
#[derive(Debug)]
pub struct Profiles;

impl Family for Profiles {
    const ID: u16 = family::PROFILES;
    const VERSION: u16 = 1;
    const CAP: u64 = crate::caps::PROFILES;
    type Request = Request;
    type Reply = Reply;
    type Event = Event;
}
