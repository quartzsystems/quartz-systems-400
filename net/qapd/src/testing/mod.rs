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
//! - profiles `BOB` (`SECRET1`) and `WARN` (`WARNPW1`, its password
//!   expiring inside the 7-day warning period);
//! - the commands in [`commands`];
//! - Telnet tickets as the Telnet server takes them
//!   ([`System::redeem_ticket`]).

pub mod commands;
mod host;

use std::collections::HashMap;
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::thread::{self, JoinHandle};
use std::time::Duration;

pub use host::MemHost;

use crate::host::Ending;
use crate::sessions::Sessions;
use crate::tickets::{self, Tickets};
use crate::{Closed, Config};

/// A lock, poison or not.
fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Holds operations in progress while closed (`now_ms` and
/// `system_info` pass through it).
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
    /// Holds `now_ms` and `system_info`.
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
    /// End the running command (a `Cancel`).
    pub cancel: Arc<AtomicBool>,
    tokens: AtomicU32,
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
            cancel: Arc::new(AtomicBool::new(false)),
            tokens: AtomicU32::new(0),
        };
        s.gate.set(true);
        s.user("BOB", "SECRET1");
        s.user("WARN", "WARNPW1");
        s.edit("WARN", |u| u.expires = Some(EPOCH + 3 * 86_400));
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
    /// variable as the client sent them, over TLS or not. Used up
    /// whatever the answer; true when the user would be signed on.
    pub fn redeem_ticket(&self, user: &str, password_var: &[u8], tls: bool) -> bool {
        let Some(t) = tickets::parse_hex(password_var) else {
            return false;
        };
        let user = user.trim().to_ascii_uppercase();
        let ok = lock(&self.tickets).redeem(&t, &user, self.now(), tls).is_ok();
        lock(&self.audit).push(format!("PS V {user}"));
        let u = self.get(&user);
        ok && self.auto_signon.load(Ordering::SeqCst) && !u.disabled && !u.none
    }

    /// A token or ticket no other has.
    fn next_token(&self, salt: u8) -> qap_proto::Token {
        let n = self.tokens.fetch_add(1, Ordering::SeqCst) + 1;
        let mut b = [salt; 32];
        b[..4].copy_from_slice(&n.to_le_bytes());
        qap_proto::Token(b)
    }
}

/// A server on loopback for one connection of `system`: the client's
/// socket and the server's thread, which answers why it closed.
///
/// # Panics
/// When loopback cannot be bound.
#[must_use]
pub fn serve_loopback(cfg: Config, system: &Arc<System>) -> (TcpStream, JoinHandle<Closed>) {
    static JOBS: AtomicU32 = AtomicU32::new(100);
    let l = TcpListener::bind("127.0.0.1:0").expect("loopback");
    let addr = l.local_addr().expect("bound");
    let sys = Arc::clone(system);
    let job = JOBS.fetch_add(1, Ordering::SeqCst);
    let server = thread::spawn(move || {
        let Ok((s, _)) = l.accept() else {
            return Closed::Broken;
        };
        let _ = s.set_read_timeout(Some(Duration::from_millis(5)));
        let mut host = MemHost::new(sys, job);
        crate::serve(s, &mut host, cfg)
    });
    let s = TcpStream::connect(addr).expect("loopback");
    (s, server)
}
