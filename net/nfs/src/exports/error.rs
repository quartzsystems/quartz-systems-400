// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What goes wrong in an option list, the flags of `EXPORTFS`, an entry
//! of `/etc/exports` or `/etc/netgroup`, as IBM's messages say it.

use thiserror::Error;

/// An option list that is not valid (`EXPORTFS -O`, `MOUNT OPTIONS`).
#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum OptError {
    /// An option not known, or not written correctly, at a position
    /// (1-based) of the list: `CPFA1B0`.
    #[error("option not known at or near position {pos}")]
    Unknown {
        /// Where, from 1.
        pos: usize,
    },
    /// An option given twice: `CPFA1B1`.
    #[error("option {name} is given more than once")]
    Duplicate {
        /// The option.
        name: String,
    },
    /// A value outside what the option takes: `CPFA1BB`.
    #[error("option {name} does not take the value {value}")]
    BadValue {
        /// The option.
        name: String,
        /// The value.
        value: String,
    },
}

impl OptError {
    /// The message identifier.
    #[must_use]
    pub fn msgid(&self) -> &'static str {
        match self {
            Self::Unknown { .. } => "CPFA1B0",
            Self::Duplicate { .. } => "CPFA1B1",
            Self::BadValue { .. } => "CPFA1BB",
        }
    }

    /// The reason code `CPDA1C5` gives an export that failed this way.
    #[must_use]
    pub fn reason(&self) -> u32 {
        match self {
            Self::Unknown { .. } => 11,
            Self::Duplicate { .. } => 12,
            Self::BadValue { .. } => 13,
        }
    }
}

/// `EXPORTFS`'s flags that do not go together.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum FlagError {
    /// A flag not known, given twice, or `-O` without its list.
    #[error(transparent)]
    Opt(#[from] OptError),
    /// `-A`, `-I`, `-O`, `-U` and `-F` combined in a way the command does
    /// not take: `CPFA1B2`.
    #[error("the flags do not go together")]
    Combination,
    /// `DIR` with `-A`: `CPFA1B3`.
    #[error("DIR is not taken with -A")]
    DirWithAll,
    /// `-I` or `-O` without `DIR`: `CPFA1C3`.
    #[error("-I and -O need DIR")]
    DirNeeded,
    /// `-U` without `DIR` or `-A`: `CPFA1C4`.
    #[error("-U needs DIR or -A")]
    UnexportWhat,
}

impl FlagError {
    /// The message identifier.
    #[must_use]
    pub fn msgid(&self) -> &'static str {
        match self {
            Self::Opt(o) => o.msgid(),
            Self::Combination => "CPFA1B2",
            Self::DirWithAll => "CPFA1B3",
            Self::DirNeeded => "CPFA1C3",
            Self::UnexportWhat => "CPFA1C4",
        }
    }
}

/// An entry of `/etc/exports` that cannot be exported, with `CPDA1C5`'s
/// reason code.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
#[error("line {line}: {detail}")]
pub struct EntryError {
    /// The line the entry starts on, from 1.
    pub line: usize,
    /// The entry's path, when it got that far.
    pub path: String,
    /// `CPDA1C5`'s reason code.
    pub reason: u32,
    /// What is wrong.
    pub detail: String,
}

/// `/etc/netgroup` that cannot be used.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum NetgroupError {
    /// A line not of the file's form (`CPDA1C5` reason 17).
    #[error("line {line}: not a netgroup entry")]
    Format {
        /// The line, from 1.
        line: usize,
    },
    /// A group that contains itself, or nests too deep (reason 8).
    #[error("netgroup {0} contains itself or nests too deep")]
    Cycle(String),
}

impl NetgroupError {
    /// `CPDA1C5`'s reason code.
    #[must_use]
    pub fn reason(&self) -> u32 {
        match self {
            Self::Format { .. } => 17,
            Self::Cycle(_) => 8,
        }
    }
}

/// `MFS` not of the form `host:path`: `CPFA1C6`.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
#[error("MFS {0} is not host:path")]
pub struct MfsError(pub String);
