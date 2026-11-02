// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A QS/400 in memory for tests, QS/400's own and QS/400 Access's: a
//! [`System`] (profiles, sessions, tickets, a few commands, a clock)
//! and a [`MemHost`] for each connection, served over loopback by
//! [`serve_loopback`]. It follows the real system's rules and
//! messages where tests look at them:
//!
//! - sign-on as a 5250 display: `QMAXSIGN` 3 with the profile
//!   disabled; `CPF1107`, `CPF1392` before the last try, `CPF1393` at
//!   it, `CPF1394` disabled, `CPF1118` `*NONE`, `CPD9BAE` expired;
//!   audit entries as `PW` types;
//! - profiles `BOB` (`SECRET1`), `WARN` (`WARNPW1`, its password
//!   expiring inside the 7-day warning period) and `ANN` (`ANNPW01`,
//!   whose spooled files `BOB` may not touch);
//! - the commands in [`commands`];
//! - Telnet tickets as the Telnet server takes them
//!   ([`System::redeem_ticket`]);
//! - output queues and spooled files ([`fixtures`]) under the rules
//!   of the spooled-file commands ([`spool`]), so a client can try its
//!   Printer Output in process.

pub mod commands;
pub mod fixtures;
mod host;
pub mod spool;

use std::collections::HashMap;
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::thread::{self, JoinHandle};

pub use host::{MemHost, MemThread};

use crate::host::Ending;
use crate::sessions::Sessions;
use crate::tickets::{self, Tickets};
use crate::{Closed, Config, Identity, Polled};
use qap_proto::types::JobId;

/// A lock, poison or not.
fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Holds operations in progress while closed (`system_info` passes
/// through it).
#[derive(Default)]
pub struct Gate {
    open: Mutex<bool>,
    cv: Condvar,
}

impl Gate {
    /// Open or close.
    pub fn set(&self, open: bool) {
        *lock(&self.open) = open;
        self.cv.notify_all();
    }

    fn pass(&self) {
        let g = lock(&self.open);
        drop(self.cv.wait_while(g, |o| !*o).unwrap_or_else(PoisonError::into_inner));
    }
}

/// A user profile.
#[derive(Clone, Debug, Default)]
pub struct User {
    /// The password.
    pub password: String,
    /// `STATUS(*DISABLED)`.
    pub disabled: bool,
    /// The password has expired.
    pub expired: bool,
    /// `PASSWORD(*NONE)`.
    pub none: bool,
    /// Sign-on attempts not valid since the last good one.
    pub invalid: u32,
    /// When the password expires, Unix seconds; `None` for never.
    pub expires: Option<u64>,
}

/// The system: what every connection shares.
pub struct System {
    /// Holds `system_info`.
    pub gate: Gate,
    /// Set to end the connections from outside.
    pub ending: Mutex<Option<Ending>>,
    /// The profiles by name.
    pub users: Mutex<HashMap<String, User>>,
    /// The sessions.
    pub sessions: Mutex<Sessions>,
    /// The Telnet tickets.
    pub tickets: Mutex<Tickets>,
    /// Unix seconds.
    pub clock: Mutex<u64>,
    /// `CHGQAPA IDLETIMO`.
    pub idle_secs: u32,
    /// `QPWDEXPWRN`.
    pub warning_days: u16,
    /// `QRMTSIGN` allows automatic sign-on.
    pub auto_signon: AtomicBool,
    /// Audit entries: `PW P BOB`, `PS P BOB`, ...
    pub audit: Mutex<Vec<String>>,
    /// Products `SystemInfo` lists.
    pub products: Mutex<usize>,
    /// Libraries `CRTLIB` made.
    pub libraries: Mutex<Vec<String>>,
    /// Who each connection job runs as, by job.
    pub running_as: Mutex<HashMap<u32, String>>,
    /// The output queues and spooled files.
    pub spool: Mutex<spool::Spool>,
    /// A panic to come, once.
    pub crash: Mutex<Option<Crash>>,
    tokens: AtomicU32,
}

/// Where a connection's next panic comes, for the tests of its close.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Crash {
    /// `Ping`, on the dispatcher's thread.
    Ping,
    /// `SystemInfo`, on a secondary thread.
    SystemInfo,
    /// A command, on the job's own thread.
    Command,
}

/// The clock's start: 2026-10-26.
pub const EPOCH: u64 = 1_793_000_000;

impl System {
    /// The shipped system: `BOB` and `WARN`.
    #[must_use]
    pub fn new() -> Arc<Self> {
        let s = Self {
            gate: Gate::default(),
            ending: Mutex::new(None),
            users: Mutex::new(HashMap::new()),
            sessions: Mutex::new(Sessions::new()),
            tickets: Mutex::new(Tickets::new()),
            clock: Mutex::new(EPOCH),
            idle_secs: 600,
            warning_days: 7,
            auto_signon: AtomicBool::new(true),
            audit: Mutex::new(Vec::new()),
            products: Mutex::new(3),
            libraries: Mutex::new(Vec::new()),
            running_as: Mutex::new(HashMap::new()),
            spool: Mutex::new(spool::Spool::default()),
            crash: Mutex::new(None),
            tokens: AtomicU32::new(0),
        };
        s.gate.set(true);
        s.user("BOB", "SECRET1");
        s.user("WARN", "WARNPW1");
        s.edit("WARN", |u| u.expires = Some(EPOCH + 3 * 86_400));
        s.user("ANN", "ANNPW01");
        fixtures::install(&mut lock(&s.spool));
        Arc::new(s)
    }

    /// Add profile `name`.
    pub fn user(&self, name: &str, password: &str) {
        lock(&self.users).insert(
            name.into(),
            User {
                password: password.into(),
                ..User::default()
            },
        );
    }

    /// Change profile `name`.
    pub fn edit(&self, name: &str, f: impl FnOnce(&mut User)) {
        if let Some(u) = lock(&self.users).get_mut(name) {
            f(u);
        }
    }

    /// Profile `name`.
    #[must_use]
    pub fn get(&self, name: &str) -> User {
        lock(&self.users).get(name).cloned().unwrap_or_default()
    }

    /// Unix seconds now.
    #[must_use]
    pub fn now(&self) -> u64 {
        *lock(&self.clock)
    }

    /// Move the clock on.
    pub fn advance(&self, secs: u64) {
        *lock(&self.clock) += secs;
    }

    /// End every connection as the server or an operator would.
    pub fn end(&self, how: Ending) {
        *lock(&self.ending) = Some(how);
    }

    /// The Telnet server's side of a ticket: `user` and the password
    /// variable as the client sent them, over TLS or not, checked as the
    /// storage service checks it ([`tickets::check`]). Used up
    /// whatever the answer; true when the user would be signed on.
    pub fn redeem_ticket(&self, user: &str, password_var: &[u8], tls: bool) -> bool {
        let Some(t) = tickets::parse_hex(password_var) else {
            return false;
        };
        let user = user.trim().to_ascii_uppercase();
        let allowed = self.auto_signon.load(Ordering::SeqCst);
        let profile_ok = |u: &str| {
            let u = self.get(u);
            !u.disabled && !u.none && !u.expired
        };
        let r = tickets::check(&mut lock(&self.tickets), &t, &user, self.now(), tls, allowed, profile_ok);
        lock(&self.audit).push(format!("PS V {user}"));
        r.is_ok()
    }

    /// Panic if `at` is the crash set; it happens once.
    fn crash_at(&self, at: Crash) {
        let mut c = lock(&self.crash);
        if *c == Some(at) {
            *c = None;
            drop(c);
            panic!("test crash at {at:?}");
        }
    }

    /// A token or ticket no other has.
    fn next_token(&self, salt: u8) -> qap_proto::Token {
        let n = self.tokens.fetch_add(1, Ordering::SeqCst) + 1;
        let mut b = [salt; 32];
        b[..4].copy_from_slice(&n.to_le_bytes());
        qap_proto::Token(b)
    }
}

/// The identity of connection job `job`: `job/QUSER/QAPCNN`, its
/// number the connection's too.
#[must_use]
pub fn identity(job: u32, peer: Option<SocketAddr>) -> Identity {
    Identity {
        connection: u64::from(job),
        job: JobId {
            name: "QAPCNN".into(),
            user: "QUSER".into(),
            number: format!("{job:06}"),
        },
        peer,
    }
}

/// A connection job number no other test connection has.
#[must_use]
pub fn next_job() -> u32 {
    static JOBS: AtomicU32 = AtomicU32::new(100);
    JOBS.fetch_add(1, Ordering::SeqCst)
}

/// A server on loopback for one connection of `system`: the client's
/// socket and the server's thread, which answers why it closed.
///
/// # Panics
/// When loopback cannot be bound.
#[must_use]
pub fn serve_loopback(cfg: Config, system: &Arc<System>) -> (TcpStream, JoinHandle<Closed>) {
    serve_loopback_as(cfg, system, next_job())
}

/// [`serve_loopback`] as connection job `job`.
///
/// # Panics
/// When loopback cannot be bound.
#[must_use]
pub fn serve_loopback_as(cfg: Config, system: &Arc<System>, job: u32) -> (TcpStream, JoinHandle<Closed>) {
    let l = TcpListener::bind("127.0.0.1:0").expect("loopback");
    let addr = l.local_addr().expect("bound");
    let sys = Arc::clone(system);
    let server = thread::spawn(move || {
        let (s, peer) = match l.accept() {
            Ok(a) => a,
            Err(e) => return Closed::Broken(format!("accept: {e}")),
        };
        // Not a read timeout: see `Polled`.
        let s = match Polled::new(s) {
            Ok(s) => s,
            Err(e) => return Closed::Broken(format!("non-blocking: {e}")),
        };
        let mut host = MemHost::new(sys, job);
        crate::serve(s, &mut host, cfg, identity(job, Some(peer)))
    });
    let s = TcpStream::connect(addr).expect("loopback");
    (s, server)
}
