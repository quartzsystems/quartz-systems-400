// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The instance job's connection loop over a service link
//! (docs/DIRSRV.md §4 "The job").
//!
//! The job holds no data: every operation goes to the storage service
//! as one of requests 1560-1567. [`Service`] is those eight requests as
//! the loop sees them; the job implements it over its `stg` link, the
//! tests over a [`crate::ops::Directory`] in memory. [`Links`] makes one
//! per connection thread.
//!
//! - [`frame`]: LDAP messages cut from a byte stream.
//! - [`conn`]: one connection: TLS, `StartTLS`, binds, operations,
//!   searches streamed with a budget, abandon, persistent searches,
//!   idle and write time-outs, the notice of disconnection.
//! - [`listen`]: the listeners on `ibm-slapdPort` and
//!   `ibm-slapdSecurePort`, a thread per connection, the counts.

pub mod conn;
pub mod frame;
pub mod listen;
mod reply;

use std::sync::atomic::{AtomicU64, Ordering};

pub use conn::Connection;
pub use frame::Framer;
pub use listen::{Listeners, Server};

/// QS/400's application ID for `QIBM_GLD_DIRSRV_SERVER`.
pub const APP_ID: &str = "QS400_DIRECTORY_SERVER";

/// What the job knows of a connection (`stg::dir::DirConn`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ConnInfo {
    /// The client's address and port.
    pub client: String,
    /// The connection is under TLS.
    pub tls: bool,
    /// The TLS client certificate's subject DN, when one was presented.
    pub client_subject: Option<String>,
}

/// What the job runs by (`stg::dir::DirSettings`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JobSettings {
    /// The instance's name.
    pub instance: String,
    /// `ibm-slapdPort`.
    pub port: u16,
    /// `ibm-slapdSecurePort`.
    pub secure_port: u16,
    /// `ibm-slapdSecurity`: `none`, `SSL`, `TLS` or `SSLTLS`.
    pub security: String,
    /// `ibm-slapdIdleTimeOut`, seconds (0: none).
    pub idle_timeout: u32,
    /// `ibm-slapdWriteTimeout`, seconds (0: none).
    pub write_timeout: u32,
    /// The certificate store's application ID.
    pub app_id: String,
    /// The largest request taken, bytes (0: `ldap::ber::MAX_LEN`).
    pub max_message: u32,
}

impl Default for JobSettings {
    fn default() -> Self {
        Self {
            instance: "QUSRDIR".into(),
            port: 389,
            secure_port: 636,
            security: "none".into(),
            idle_timeout: 300,
            write_timeout: 120,
            app_id: APP_ID.into(),
            max_message: 0,
        }
    }
}

impl JobSettings {
    /// The frame limit.
    #[must_use]
    pub fn max_len(&self) -> usize {
        if self.max_message == 0 {
            ldap::ber::MAX_LEN
        } else {
            self.max_message as usize
        }
    }

    /// Whether the secure port listens (`SSL`, `SSLTLS`), given a
    /// certificate.
    #[must_use]
    pub fn ssl(&self) -> bool {
        matches!(
            self.security.to_ascii_uppercase().as_str(),
            "SSL" | "SSLTLS"
        )
    }

    /// Whether `StartTLS` is taken on the port in the clear (`TLS`,
    /// `SSLTLS`), given a certificate.
    #[must_use]
    pub fn start_tls(&self) -> bool {
        matches!(
            self.security.to_ascii_uppercase().as_str(),
            "TLS" | "SSLTLS"
        )
    }
}

/// What `dir_open` answers.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Opened {
    /// The session.
    pub session: u64,
    /// The settings.
    pub settings: JobSettings,
}

/// How much one search or next may answer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Budget {
    /// Entries and references.
    pub entries: u32,
    /// Bytes of entries.
    pub bytes: u32,
}

impl Default for Budget {
    fn default() -> Self {
        Self {
            entries: 100,
            bytes: 256 * 1024,
        }
    }
}

/// Where a request leaves its operation.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum State {
    /// Answered in full.
    #[default]
    Done,
    /// More entries: ask `next` with this cursor.
    More(u64),
    /// A page of paged results is over.
    Paused(u64),
    /// A persistent search watches.
    Watching(u64),
}

/// One request's answer.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Answer {
    /// Whole messages to send, in order.
    pub replies: Vec<Vec<u8>>,
    /// What follows.
    pub state: State,
    /// `StartTLS` succeeded: handshake after the reply.
    pub start_tls: bool,
    /// After a bind: the DN bound as.
    pub bound: Option<String>,
}

/// What `close` ends.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Close {
    /// The search of this message ID.
    Message(i32),
    /// The session.
    Session,
}

/// The job's connection counts.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Counts {
    /// Open now.
    pub current: u64,
    /// Accepted since the job started.
    pub total: u64,
    /// Of those, the ones that started TLS with `StartTLS`.
    pub tls: u64,
    /// Of those, the ones accepted on the secure port.
    pub ssl: u64,
}

/// [`Counts`] kept by the threads.
#[derive(Debug, Default)]
pub struct Counters {
    current: AtomicU64,
    total: AtomicU64,
    tls: AtomicU64,
    ssl: AtomicU64,
}

impl Counters {
    /// The counts now.
    #[must_use]
    pub fn get(&self) -> Counts {
        Counts {
            current: self.current.load(Ordering::Relaxed),
            total: self.total.load(Ordering::Relaxed),
            tls: self.tls.load(Ordering::Relaxed),
            ssl: self.ssl.load(Ordering::Relaxed),
        }
    }

    fn opened(&self, ssl: bool) {
        self.current.fetch_add(1, Ordering::Relaxed);
        self.total.fetch_add(1, Ordering::Relaxed);
        if ssl {
            self.ssl.fetch_add(1, Ordering::Relaxed);
        }
    }

    fn closed(&self) {
        self.current.fetch_sub(1, Ordering::Relaxed);
    }

    fn started_tls(&self) {
        self.tls.fetch_add(1, Ordering::Relaxed);
    }
}

/// What `changes` answers.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Changes {
    /// Search result entries, each with its search's message ID.
    pub replies: Vec<Vec<u8>>,
    /// Persistent searches the service no longer has.
    pub ended: Vec<i32>,
}

/// A request refused, or the link gone.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SvcError {
    /// The message ID (`GLDxxxx`, `CPFxxxx`).
    pub msgid: String,
    /// The text.
    pub text: String,
    /// The link to the service is gone: the connection ends.
    pub broken: bool,
}

impl SvcError {
    /// A refusal.
    pub fn new(msgid: &str, text: impl Into<String>) -> Self {
        Self {
            msgid: msgid.into(),
            text: text.into(),
            broken: false,
        }
    }

    /// The link is gone.
    pub fn broken(text: impl Into<String>) -> Self {
        Self {
            broken: true,
            ..Self::new("", text)
        }
    }
}

impl std::fmt::Display for SvcError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.msgid.is_empty() {
            f.write_str(&self.text)
        } else {
            write!(f, "{} {}", self.msgid, self.text)
        }
    }
}

/// A service result.
pub type SvcResult<T> = Result<T, SvcError>;

/// The storage service's directory requests, as one thread asks them.
pub trait Service {
    /// 1560: a connection opened.
    ///
    /// # Errors
    /// The service's.
    fn open(&mut self, instance: &str, conn: &ConnInfo) -> SvcResult<Opened>;
    /// 1561: a bind request's bytes.
    ///
    /// # Errors
    /// The service's.
    fn bind(&mut self, session: u64, msg: &[u8], conn: &ConnInfo) -> SvcResult<Answer>;
    /// 1562: any other operation's bytes.
    ///
    /// # Errors
    /// The service's.
    fn op(&mut self, session: u64, msg: &[u8]) -> SvcResult<Answer>;
    /// 1563: a search request's bytes.
    ///
    /// # Errors
    /// The service's.
    fn search(&mut self, session: u64, msg: &[u8], budget: Budget) -> SvcResult<Answer>;
    /// 1564: a cursor's next entries.
    ///
    /// # Errors
    /// The service's.
    fn next(&mut self, session: u64, cursor: u64, budget: Budget) -> SvcResult<Answer>;
    /// 1565: a search or the session ended.
    ///
    /// # Errors
    /// The service's.
    fn close(&mut self, session: u64, what: Close) -> SvcResult<()>;
    /// 1566: the job's counts.
    ///
    /// # Errors
    /// The service's.
    fn conns(&mut self, instance: &str, counts: Counts) -> SvcResult<()>;
    /// 1567: what the session's persistent searches saw.
    ///
    /// # Errors
    /// The service's.
    fn changes(&mut self, session: u64, max: u32) -> SvcResult<Changes>;
}

/// Makes a [`Service`] for the calling thread.
pub trait Links: Send + Sync + 'static {
    /// The thread's service.
    type S: Service;
    /// A link for this thread.
    ///
    /// # Errors
    /// No link could be made.
    fn link(&self) -> Result<Self::S, String>;
}
