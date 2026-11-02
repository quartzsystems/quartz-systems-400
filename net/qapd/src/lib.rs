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
//!   to the job's own thread, the rest to secondary threads.
//! - `conn`: the job's own thread: commands one at a time, the events;
//!   what the threads share.
//! - [`host`]: what the connection reaches of the system.
//! - [`spool`]: `ReadSpooledFile` over a file's data.
//! - [`sessions`], [`tickets`]: the storage service's tables.
//! - [`testing`]: a system in memory for tests, QS/400 Access's too.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod frames;
pub mod host;
pub mod sessions;
pub mod spool;
pub mod testing;
pub mod tickets;

mod conn;
mod lanes;
mod pump;

use std::io::{Read, Write};
use std::net::{Shutdown, TcpStream};
use std::time::Duration;

pub use conn::serve;
pub use host::{Canceller, Ending, Host, Ran, Refusal, SystemId, ThreadHost, Threads};

/// A connection's byte stream. Reads must not block for long (a read
/// timeout of a few milliseconds): the pump also writes and watches the
/// idle time between reads.
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
    /// Close after this long without a frame.
    pub idle_close: Duration,
    /// Requests outstanding at once; beyond, `Busy`.
    pub max_outstanding: usize,
    /// How often the connection's thread looks for an end from outside.
    pub poll: Duration,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            idle_close: qap_proto::IDLE_CLOSE,
            max_outstanding: qap_proto::MAX_OUTSTANDING,
            poll: Duration::from_millis(100),
        }
    }
}

/// Why a connection closed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Closed {
    /// The client closed it.
    Peer,
    /// No frame for [`Config::idle_close`].
    Idle,
    /// A frame over 1 MiB or one that does not decode.
    BadFrame,
    /// A read or write failed.
    Broken,
    /// `Hello` refused: misplaced, repeated, or no common version.
    Refused,
    /// The server is ending.
    ServerEnding,
    /// An operator ended the job.
    Operator,
}
