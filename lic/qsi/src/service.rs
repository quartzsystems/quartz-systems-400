// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The service family: a managed system's problems. A problem is
//! something a person should look at, one record per condition (not per
//! reference code posted): when it was first and last seen and how
//! often, the code, where it came from, the resources involved, whether
//! the condition still holds, and the analysis log. List, get, close;
//! events when one opens, is seen again, clears and closes.
//!
//! The guided text for a reference code is not QSI's: it is the
//! console's.
//!
//! Version 2 ([`crate::caps::PROBLEM_NOTES`]) adds a person's notes to
//! a problem's analysis log, and its severity: a list and a get of
//! their own that carry it beside the summary ([`Rated`]), since
//! [`Summary`] is in a list and cannot gain a field.

use ipc::wire_struct;

use crate::base::Refcode;
use crate::host::model::{MAX_ID, MAX_TEXT, check_name};
use crate::{Family, family, messages, open_enum};

open_enum! {
    /// Whether a problem is open.
    ProblemStatus {
        /// Open: someone should look at it.
        Open = 1,
        /// Closed by a person.
        Closed = 2,
    }
}

open_enum! {
    /// What a problem came from.
    SourceKind {
        /// The managed system itself.
        System = 1,
        /// One of a host's partitions.
        Partition = 2,
    }
}

open_enum! {
    /// What a resource involved in a problem is.
    ResourceKind {
        /// A disk unit.
        DiskUnit = 1,
        /// A storage pool.
        StoragePool = 2,
        /// A virtual disk.
        VirtualDisk = 3,
        /// A network interface.
        NetworkInterface = 4,
        /// A virtual switch.
        VirtualSwitch = 5,
        /// A partition.
        Partition = 6,
        /// A slot of the system's own code (`A`, `B`).
        CodeSlot = 7,
        /// The system as a whole.
        System = 8,
    }
}

/// Where a problem came from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Source {
    /// The system itself, or a partition.
    pub kind: SourceKind,
    /// The partition's ID; 0 for the system.
    pub id: u32,
    /// The partition's name; the system's for the system.
    pub name: String,
}
wire_struct!(Source { kind, id, name });

/// A resource involved in a problem.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Resource {
    /// What it is.
    pub kind: ResourceKind,
    /// Its name (a pool's, a partition's, a slot's); empty when it has
    /// none.
    pub name: String,
    /// Its location code; empty when it has none.
    pub location: String,
    /// Its serial number; empty when it has none.
    pub serial: String,
}
wire_struct!(Resource {
    kind,
    name,
    location,
    serial
});

/// A line of a problem's analysis log.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Line {
    /// When, seconds since the epoch.
    pub time: u64,
    /// The line, in the managed side's words.
    pub text: String,
}
wire_struct!(Line { time, text });

/// What a list shows of a problem, and what its events carry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Summary {
    /// The managed system's number for it: ascending, never reused.
    pub number: u64,
    /// Open or closed.
    pub status: ProblemStatus,
    /// When it was first seen, seconds since the epoch.
    pub first_seen: u64,
    /// When it was last seen, seconds since the epoch.
    pub last_seen: u64,
    /// How many times it was seen.
    pub count: u32,
    /// The reference code, as last posted for it, with its words.
    pub refcode: Refcode,
    /// Where it came from.
    pub source: Source,
    /// One line saying what it is.
    pub description: String,
    /// The message identifier that describes it, the managed side's.
    pub msgid: String,
    /// Whether the condition still holds.
    pub holds: bool,
    /// Who closed it; empty while open.
    pub closed_by: String,
    /// When it was closed, seconds since the epoch; 0 while open.
    pub closed_at: u64,
}
wire_struct!(Summary {
    number,
    status,
    first_seen,
    last_seen,
    count,
    refcode,
    source,
    description,
    msgid,
    holds,
    closed_by,
    closed_at
});

/// A problem whole.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Problem {
    /// What a list shows of it.
    pub summary: Summary,
    /// The resources involved.
    pub resources: Vec<Resource>,
    /// The analysis log, oldest first.
    pub log: Vec<Line>,
}
wire_struct!(Problem {
    summary,
    resources,
    log
});

/// Which problems a list gives; every field that is set must match.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Filter {
    /// Open or closed only.
    pub status: Option<ProblemStatus>,
    /// From this source only, matched by kind and ID (the name is not
    /// compared).
    pub source: Option<Source>,
    /// Seen last at or after this time, seconds since the epoch.
    pub since: Option<u64>,
}
wire_struct!(Filter {
    status,
    source,
    since
});

/// List problems, newest first.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct List {
    /// Which.
    pub filter: Filter,
    /// Problems numbered below this; `None` from the newest.
    pub before: Option<u64>,
    /// At most this many; the managed side may give fewer.
    pub limit: u32,
}
wire_struct!(List {
    filter,
    before,
    limit
});

/// A page of problems.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Page {
    /// The problems, newest first.
    pub problems: Vec<Summary>,
    /// Where the next page starts (`before`), when there is one.
    pub more: Option<u64>,
}
wire_struct!(Page { problems, more });

/// Get one problem whole.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Get {
    /// Its number.
    pub number: u64,
}
wire_struct!(Get { number });

/// Close a problem. The request's user is who closed it. Refused with
/// [`msgid::STILL_HOLDS`] while the condition holds, unless `anyway`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Close {
    /// Its number.
    pub number: u64,
    /// Close it even though the condition holds.
    pub anyway: bool,
}
wire_struct!(Close { number, anyway });

/// What an event says of a problem.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Noticed {
    /// The problem as it is now.
    pub problem: Summary,
    /// When, seconds since the epoch.
    pub time: u64,
}
wire_struct!(Noticed { problem, time });

open_enum! {
    /// How grave a problem is (version 2).
    Severity {
        /// Someone should act: the system goes on, short of something.
        Attention = 1,
        /// The system, or the part of it named, stopped.
        Terminal = 2,
        /// For the record: nothing needs doing.
        Information = 3,
    }
}

/// A problem as a list shows it, with its severity (version 2).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rated {
    /// How grave it is.
    pub severity: Severity,
    /// The rest, as version 1's list shows it.
    pub summary: Summary,
}
wire_struct!(Rated { severity, summary });

/// A page of problems with their severities (version 2).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RatedPage {
    /// The problems, newest first.
    pub problems: Vec<Rated>,
    /// Where the next page starts (`before`), when there is one.
    pub more: Option<u64>,
}
wire_struct!(RatedPage { problems, more });

/// A problem whole, with its severity (version 2).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RatedProblem {
    /// How grave it is.
    pub severity: Severity,
    /// The problem.
    pub problem: Problem,
}
wire_struct!(RatedProblem { severity, problem });

/// List problems with their severities: as [`List`] (version 2).
pub type ListRated = List;

/// Get one problem whole with its severity: as [`Get`] (version 2).
pub type GetRated = Get;

/// The longest note, bytes.
pub const MAX_NOTE: usize = MAX_TEXT;

/// Add a note to a problem's analysis log (version 2). The request's
/// user is its author; a closed problem takes none
/// ([`msgid::CLOSED`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Note {
    /// The problem's number.
    pub number: u64,
    /// The note, 1 to [`MAX_NOTE`] bytes.
    pub text: String,
}
wire_struct!(Note { number, text });

/// A note was added to a problem (version 2).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Noted {
    /// The problem's number.
    pub number: u64,
    /// The line as written into its analysis log.
    pub line: Line,
    /// The console's user who wrote it.
    pub by: String,
    /// When, seconds since the epoch.
    pub time: u64,
}
wire_struct!(Noted {
    number,
    line,
    by,
    time
});

/// Operation numbers.
pub mod op {
    /// [`super::List`].
    pub const LIST: u16 = 0x0001;
    /// [`super::Get`].
    pub const GET: u16 = 0x0002;
    /// [`super::Close`].
    pub const CLOSE: u16 = 0x0003;
    /// [`super::Note`] ([`crate::caps::PROBLEM_NOTES`]).
    pub const NOTE: u16 = 0x0004;
    /// [`super::ListRated`] ([`crate::caps::PROBLEM_NOTES`]).
    pub const LIST_RATED: u16 = 0x0005;
    /// [`super::GetRated`] ([`crate::caps::PROBLEM_NOTES`]).
    pub const GET_RATED: u16 = 0x0006;

    /// Whether `op` is version 2's, which needs
    /// [`crate::caps::PROBLEM_NOTES`].
    #[must_use]
    pub fn is_v2(op: u16) -> bool {
        (NOTE..=GET_RATED).contains(&op)
    }
}

/// Event numbers.
pub mod event {
    /// A problem was opened.
    pub const OPENED: u16 = 0x0001;
    /// An open problem was seen again.
    pub const SEEN: u16 = 0x0002;
    /// A problem's condition no longer holds.
    pub const CLEARED: u16 = 0x0003;
    /// A problem was closed.
    pub const CLOSED: u16 = 0x0004;
    /// A note was added ([`crate::caps::PROBLEM_NOTES`]).
    pub const NOTED: u16 = 0x0005;

    /// Whether `op` is version 2's, which needs
    /// [`crate::caps::PROBLEM_NOTES`].
    #[must_use]
    pub fn is_v2(op: u16) -> bool {
        op == NOTED
    }
}

/// The family's own refusals (`QSI1001` to `QSI1099`).
pub mod msgid {
    /// A close while the condition still holds, without `anyway`.
    pub const STILL_HOLDS: &str = "QSI1001";
    /// No problem of that number, from a managed side with no
    /// identifier of its own.
    pub const NO_PROBLEM: &str = "QSI1002";
    /// A close of a problem closed already, or a note to one.
    pub const CLOSED: &str = "QSI1003";
}

messages! {
    /// What a console asks.
    pub enum Request: family::SERVICE => {
        /// A page of problems.
        0x0001 => List(List),
        /// One problem whole.
        0x0002 => Get(Get),
        /// Close one.
        0x0003 => Close(Close),
        /// Add a note to one.
        0x0004 => Note(Note),
        /// A page of problems with their severities.
        0x0005 => ListRated(ListRated),
        /// One problem whole with its severity.
        0x0006 => GetRated(GetRated),
    }
}

messages! {
    /// What the managed side answers.
    pub enum Reply: family::SERVICE => {
        /// A page of problems.
        0x0001 => List(Page),
        /// The problem.
        0x0002 => Get(Problem),
        /// Closed: the problem as it is now.
        0x0003 => Close(Summary),
        /// The line as written into the analysis log.
        0x0004 => Note(Line),
        /// A page of problems with their severities.
        0x0005 => ListRated(RatedPage),
        /// The problem with its severity.
        0x0006 => GetRated(RatedProblem),
    }
}

messages! {
    /// What the managed side pushes.
    pub enum Event: family::SERVICE => {
        /// A problem was opened.
        0x0001 => Opened(Noticed),
        /// An open problem was seen again.
        0x0002 => Seen(Noticed),
        /// Its condition no longer holds.
        0x0003 => Cleared(Noticed),
        /// It was closed.
        0x0004 => Closed(Noticed),
        /// A note was added to it.
        0x0005 => Noted(Noted),
    }
}

impl Request {
    /// The protocol's bounds.
    pub fn check(&self) -> Result<(), &'static str> {
        match self {
            Self::List(l) | Self::ListRated(l) => match &l.filter.source {
                Some(s) if s.id > MAX_ID => Err("a partition ID over 999"),
                Some(s) => check_name(&s.name),
                None => Ok(()),
            },
            Self::Get(Get { number })
            | Self::GetRated(Get { number })
            | Self::Close(Close { number, .. })
            | Self::Note(Note { number, .. })
                if *number == 0 =>
            {
                Err("problem 0")
            }
            Self::Note(n) if n.text.is_empty() => Err("an empty note"),
            Self::Note(n) if n.text.len() > MAX_NOTE => Err("a note longer than 256 bytes"),
            Self::Get(_) | Self::GetRated(_) | Self::Close(_) | Self::Note(_) => Ok(()),
        }
    }
}

/// The service family. Version 2 ([`crate::caps::PROBLEM_NOTES`]) adds
/// notes and severity; version 1 is [`crate::caps::SERVICE`].
#[derive(Debug)]
pub struct Service;

impl Family for Service {
    const ID: u16 = family::SERVICE;
    const VERSION: u16 = 2;
    const CAP: u64 = crate::caps::SERVICE;
    type Request = Request;
    type Reply = Reply;
    type Event = Event;
}
