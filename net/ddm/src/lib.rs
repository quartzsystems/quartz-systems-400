// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! DDM and DRDA for QS/400: the datastream (DSS headers, chaining and
//! correlation, LL/CP objects), the code points, FD:OCA descriptors and
//! data for the SQL types of the database, the security mechanisms, the
//! trace files the recording proxy writes, the application server
//! (`server::Session`) and requester (`client::Connection`) as library
//! code, and the `*RDB` directory, `*DDMF` and DDM TCP attributes.

#![deny(missing_docs)]

/// Declare named `u16` constants and their `ALL` table.
macro_rules! consts {
    ($( $(#[$m:meta])* $name:ident = $v:expr ),* $(,)?) => {
        $( $(#[$m])* pub const $name: u16 = $v; )*
        /// Every constant here, by name.
        pub const ALL: &[(&str, u16)] = &[ $( (stringify!($name), $v) ),* ];
    };
}

pub mod client;
pub mod cp;
pub mod fdoca;
pub mod file;
pub mod msg;
pub mod objects;
pub mod sec;
pub mod server;
pub mod sql;
pub mod trace;
pub mod wire;

pub use wire::{Dss, DssType, Object};

/// Which way a DSS goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Dir {
    /// Requester to server (`>` in traces).
    Request,
    /// Server to requester (`<`).
    Reply,
}

impl Dir {
    /// Its trace marker.
    pub fn marker(self) -> char {
        match self {
            Self::Request => '>',
            Self::Reply => '<',
        }
    }
}

/// What can go wrong.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum Error {
    /// Fewer bytes than a structure needs.
    #[error("{what}: need {need} bytes, have {have}")]
    Short {
        /// The structure.
        what: &'static str,
        /// Bytes needed.
        need: usize,
        /// Bytes there.
        have: usize,
    },
    /// A DSS header that does not read.
    #[error("not a DSS: {0}")]
    Dss(String),
    /// A required object or parameter is missing.
    #[error("missing {0}")]
    Missing(&'static str),
    /// A value the datastream does not allow.
    #[error("{0}")]
    Invalid(String),
    /// A trace file line that does not read.
    #[error("trace line {line}: {what}")]
    Trace {
        /// One-based line number.
        line: usize,
        /// What is wrong.
        what: String,
    },
    /// The partner answered with a reply message we did not want.
    #[error("{name} (severity {svrcod})")]
    Reply {
        /// The reply message's name.
        name: String,
        /// Its `SVRCOD`.
        svrcod: u16,
    },
    /// The connection failed.
    #[error("connection: {0}")]
    Io(String),
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e.to_string())
    }
}

/// Big-endian `u16` at `at`.
pub(crate) fn be16(b: &[u8], at: usize) -> u16 {
    u16::from_be_bytes([b[at], b[at + 1]])
}
