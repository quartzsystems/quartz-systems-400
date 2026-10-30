// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The host servers: the request loop of each server IBM i Access and
//! `JTOpen` connect to, over the `hostsvr` codec. A server is a
//! [`Handler`]: one request frame in, its reply frames out. The work
//! behind each request (signing on, running a command, SQL, the IFS,
//! spooled files) goes to a backend trait the job implements over its
//! link to the storage service; tests implement them in memory.

#![deny(missing_docs)]

pub mod central;
pub mod database;
pub mod dtaq;
pub mod exits;
pub mod file;
pub mod io;
pub mod jobs;
pub mod netprt;
pub mod reply;
pub mod rmtcmd;
pub mod session;
pub mod signon;
pub mod svrmap;
pub mod text;

pub use exits::{Exits, NoExits};
pub use session::{Auth, Proof, Session};

/// The VRM the servers present (`0x00VVRRMM`): IBM i 7.1, as the
/// primary oracle answers. Clients pick datastream features by it.
pub const SERVER_VRM: u32 = 0x0007_0100;

/// The datastream level the servers speak.
pub const SERVER_LEVEL: u16 = 10;

/// What a request produced.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Outcome {
    /// Reply frames, in order (none for requests without a reply).
    pub replies: Vec<Vec<u8>>,
    /// Whether the server closes the connection after them.
    pub close: bool,
}

impl Outcome {
    /// One reply.
    pub fn one(frame: Vec<u8>) -> Self {
        Self {
            replies: vec![frame],
            close: false,
        }
    }

    /// No reply.
    pub fn none() -> Self {
        Self::default()
    }

    /// Close without a reply.
    pub fn close() -> Self {
        Self {
            replies: Vec::new(),
            close: true,
        }
    }
}

/// A server's request loop, one frame at a time.
pub trait Handler {
    /// Answer one whole request frame.
    fn handle(&mut self, frame: &[u8]) -> Outcome;

    /// The connection ended, however it ended.
    fn finish(&mut self) {}
}
