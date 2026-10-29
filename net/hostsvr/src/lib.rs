// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The host server datastream that IBM i Access clients and `JTOpen`
//! speak: the 20-byte header, LL/CP parameters, the per-server request
//! and reply templates, password substitutes for every password level,
//! the port mapper, and the trace files the recording proxy writes.
//! A codec only: the servers come later.

#![deny(missing_docs)]

/// Declare named `u16` constants and their `ALL` table.
#[macro_export]
#[doc(hidden)]
macro_rules! consts {
    ($( $(#[$m:meta])* $name:ident = $v:expr ),* $(,)?) => {
        $( $(#[$m])* pub const $name: u16 = $v; )*
        /// Every constant here, by name.
        pub const ALL: &[(&str, u16)] = &[ $( (stringify!($name), $v) ),* ];
    };
}

/// Declare named `u32` constants and their `ALL` table.
#[macro_export]
#[doc(hidden)]
macro_rules! consts32 {
    ($( $(#[$m:meta])* $name:ident = $v:expr ),* $(,)?) => {
        $( $(#[$m])* pub const $name: u32 = $v; )*
        /// Every constant here, by name.
        pub const ALL: &[(&str, u32)] = &[ $( (stringify!($name), $v) ),* ];
    };
}

pub mod central;
pub mod codec;
pub mod common;
pub mod cp;
pub mod crypt;
pub mod database;
pub mod dtaq;
pub mod file;
pub mod frame;
pub mod message;
pub mod netprt;
pub mod rle;
pub mod rmtcmd;
pub mod server;
pub mod signon;
pub mod svrmap;
pub mod template;
pub mod trace;

pub use codec::{Body, Message};
pub use frame::{Frame, Header, Param};
pub use server::ServerId;

/// Which way a frame goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Dir {
    /// Client to server (`>` in traces).
    Request,
    /// Server to client (`<`).
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

/// What can go wrong decoding.
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
    /// The header's length disagrees with the frame.
    #[error("header says {said} bytes, frame has {have}")]
    Length {
        /// The header's length.
        said: usize,
        /// The frame's.
        have: usize,
    },
    /// A trace file line that does not read.
    #[error("trace line {line}: {what}")]
    Trace {
        /// One-based line number.
        line: usize,
        /// What is wrong.
        what: String,
    },
    /// A value outside what the datastream allows.
    #[error("{0}")]
    Invalid(String),
}
