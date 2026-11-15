// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The reference codes family: a partition's code history, addressed by
//! partition ID, and each code as it is posted. Each entry says whose
//! it is: the system's, from its service channel, or the host's about
//! the partition. The code and its text are passed as the host gives
//! them.

use ipc::wire_struct;

use super::model::{PartitionId, check_id};
use crate::base::Refcode;
use crate::{Family, family, messages, open_enum};

open_enum! {
    /// Who posted a code.
    Source {
        /// The host, about the partition.
        Host = 1,
        /// The system in it, on its service channel.
        System = 2,
    }
}

/// A code of a partition's history.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Posted {
    /// The code: its place in the history, its words, its text, when.
    pub refcode: Refcode,
    /// Who posted it.
    pub source: Source,
}
wire_struct!(Posted { refcode, source });

/// Read a partition's history, newest first.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct History {
    /// The partition.
    pub id: PartitionId,
    /// Codes before this place in the history; `None` from the newest.
    pub before: Option<u64>,
    /// At most this many; the host may give fewer.
    pub limit: u32,
}
wire_struct!(History { id, before, limit });

/// A page of the history.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Page {
    /// The codes, newest first.
    pub codes: Vec<Posted>,
    /// Where the next page starts (`before`), when there is one.
    pub more: Option<u64>,
}
wire_struct!(Page { codes, more });

/// A partition posted a code.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Code {
    /// The partition.
    pub id: PartitionId,
    /// Its name.
    pub name: String,
    /// The code.
    pub posted: Posted,
    /// Whether it is the partition's current code now (a note of the
    /// history is not).
    pub current: bool,
}
wire_struct!(Code {
    id,
    name,
    posted,
    current
});

/// Operation numbers.
pub mod op {
    /// [`super::History`].
    pub const HISTORY: u16 = 0x0001;
}

/// Event numbers.
pub mod event {
    /// [`super::Code`].
    pub const CODE: u16 = 0x0001;
}

/// The family's own refusals (`QSI0501` to `QSI0549`): none in
/// version 1.
pub mod msgid {}

messages! {
    /// What a console asks.
    pub enum Request: family::REFCODES => {
        /// A page of a partition's history.
        0x0001 => History(History),
    }
}

messages! {
    /// What the host answers.
    pub enum Reply: family::REFCODES => {
        /// The page.
        0x0001 => History(Page),
    }
}

messages! {
    /// What the host pushes.
    pub enum Event: family::REFCODES => {
        /// A code was posted.
        0x0001 => Code(Code),
    }
}

impl Request {
    /// The protocol's bounds.
    pub fn check(&self) -> Result<(), &'static str> {
        let Self::History(h) = self;
        check_id(h.id)
    }
}

/// The reference codes family.
#[derive(Debug)]
pub struct Refcodes;

impl Family for Refcodes {
    const ID: u16 = family::REFCODES;
    const VERSION: u16 = 1;
    const CAP: u64 = crate::caps::REFCODES;
    type Request = Request;
    type Reply = Reply;
    type Event = Event;
}
