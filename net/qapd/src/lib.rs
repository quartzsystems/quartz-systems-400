// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The QAP server's connections (docs/QAP.md), without the store or the
//! job runtime: a connection job hands [`serve`] its TLS stream and a
//! [`Host`].
//!
//! - [`frames`]: frames from a stream that arrives in pieces.
//! - `pump`: the thread that owns the stream: reads, limits, cancels,
//!   writes, the idle close.
//! - `lanes`: where requests go: `Hello` and `Ping` at once, commands
//!   and `Sql` 1 to 8 to the job's own thread (the lane), the rest to
//!   secondary threads.
//! - `conn`: the job's own thread: commands one at a time, the events;
//!   what the threads share.
//! - `watch`: the one line logged at each close, and the threads'
//!   panics as its reason.
//! - `watches`: the watches of message queues and spooled files
//!   (version 6) and the thread that sends their events.
//! - `polled`: a TCP stream without a read timeout, for loopback.
//! - [`host`]: what the connection reaches of the system.
//! - [`spool`]: `ReadSpooledFile` over a file's data.
//! - [`files`]: the `Ifs` service over an `IfsClient`.
//! - [`sql`]: the `Sql` service's answers and cancel rules.
//! - [`manage`]: the `Manage` service (version 7, draft) and its exit point.
//! - [`develop`], [`jobs`]: the `Develop` and `Jobs` services (version 8,
//!   draft).
//! - [`debug`]: the `Debug` service (version 9, draft) and its events.
//! - [`netserver`]: the `NetServer` service (version 10, draft) over the
//!   `QZLS*` APIs.
//! - [`nfs`]: the `Nfs` service (version 11, draft) over the NFS
//!   commands and the storage service's tables.
//! - [`sessions`], [`tickets`]: the storage service's tables.
//! - [`testing`]: a system in memory for tests, QS/400 Access's too.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod debug;
pub mod develop;
pub mod files;
pub mod frames;
pub mod host;
pub mod jobs;
pub mod manage;
pub mod netserver;
pub mod nfs;
pub mod sessions;
pub mod spool;
pub mod sql;
pub mod testing;
pub mod tickets;

mod conn;
mod lanes;
mod polled;
mod pump;
mod watch;
mod watches;

use std::fmt;
use std::io::{Read, Write};
use std::net::{Shutdown, TcpStream};
use std::time::Duration;

pub use conn::serve;
pub use develop::Compiled;
pub use host::{Canceller, Ending, Host, Ran, Refusal, SystemId, ThreadHost, Threads};
pub use polled::Polled;
pub use watch::{Identity, exiting};

/// A connection's byte stream. Reads must not block for long (a few
/// milliseconds): the pump also writes and watches the idle time
/// between reads. On Windows use [`Polled`], not a read timeout.
pub trait Transport: Read + Write + Send + 'static {
    /// Close, telling the peer (TLS `close_notify`).
    fn close(&mut self) {}
}

impl Transport for TcpStream {
    fn close(&mut self) {
        let _ = self.shutdown(Shutdown::Both);
    }
}

/// The limits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Config {
    /// Close after this long without a frame either way while nothing
    /// is outstanding.
    pub idle_close: Duration,
    /// Requests outstanding at once; beyond, `Busy`.
    pub max_outstanding: usize,
    /// How often the connection's thread looks for an end from outside.
    pub poll: Duration,
    /// How often the watches look at their queues and targets.
    pub watch_poll: Duration,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            idle_close: qap_proto::IDLE_CLOSE,
            max_outstanding: qap_proto::MAX_OUTSTANDING,
            poll: Duration::from_millis(100),
            watch_poll: Duration::from_millis(250),
        }
    }
}

/// Why a connection closed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Closed {
    /// The client closed it.
    Peer,
    /// No frame for [`Config::idle_close`] with nothing outstanding.
    Idle,
    /// A frame over 1 MiB or one that does not decode.
    BadFrame,
    /// A read or write failed: the error.
    Broken(String),
    /// `Hello` refused: misplaced, repeated, or no common version.
    Refused,
    /// The server is ending.
    ServerEnding,
    /// An operator ended the job.
    Operator,
    /// Another connection of the session signed off, or an operator
    /// ended another of its jobs.
    SessionEnded,
    /// One of the connection's threads panicked: which, and the panic.
    Panic(String),
    /// The job's process is ending under the connection: why.
    Exit(String),
}

impl fmt::Display for Closed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Closed::Peer => f.write_str("client closed"),
            Closed::Idle => f.write_str("idle timeout"),
            Closed::BadFrame => f.write_str("protocol error: bad frame"),
            Closed::Broken(e) => write!(f, "I/O error: {e}"),
            Closed::Refused => f.write_str("protocol error: Hello refused"),
            Closed::ServerEnding => f.write_str("server ending"),
            Closed::Operator => f.write_str("job ended by an operator"),
            Closed::Panic(p) => write!(f, "panic: {p}"),
            Closed::SessionEnded => f.write_str("session ended on another connection"),
            Closed::Exit(why) => write!(f, "job ending: {why}"),
        }
    }
}
