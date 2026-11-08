// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A QS/400 in memory for tests, QS/400's own and QS/400 Access's: a
//! [`System`] (profiles, sessions, tickets, a few commands, a clock)
//! and a [`MemHost`] for each connection, served over loopback by
//! [`serve_loopback`]. It follows the real system's rules and
//! messages where tests look at them:
//!
//! - sessions with up to 16 connections each (`Attach`), and
//!   `SessionEnded` on the others when one signs off;
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
//!   Printer Output in process;
//! - a database ([`sql`]): schemas, tables of every type, journaled and
//!   not, slow and large ones, one `BOB` may not read, for the `Sql`
//!   service;
//! - message queues ([`messages`]): the profiles' own and `QSYSOPR`
//!   with messages of every type, inquiries with valid replies and a
//!   default, more than a page, a queue `BOB` may not use, one held in
//!   break mode; [`System::inject_message`] puts a message on any queue
//!   for event tests;
//! - spooled files changed from outside, for event tests:
//!   [`System::spool_add`], [`System::spool_change`],
//!   [`System::spool_remove`];
//! - an integrated file system ([`files`]): QS/400's own `ifs` crate in
//!   memory, with home directories, a directory of 1,200 files, text in
//!   several CCSIDs (one mixed), links, a directory `BOB` may not use, a
//!   file another job holds, `/QSYS.LIB` and a 200 MiB file made on
//!   demand, for the `Ifs` service;
//! - management ([`manage`], version 7, draft): subsystems, system values,
//!   PTFs, disk units, hardware and the IPL; profiles `OPER` (`OPERPW1`,
//!   `*JOBCTL`) and `SECOFR` (`SECPW01`, every special authority) beside
//!   `BOB`, who has none; an exit program tests set
//!   ([`manage::Manage::exit_refuses`]);
//! - development ([`develop`], version 8, draft): each job's library
//!   list, libraries, objects and source members with dates, a member
//!   another job holds, one over 1 MiB, a binary source file;
//! - jobs ([`jobs`], version 8): `BOB`'s active, queued and ended jobs
//!   with their logs, `ANN`'s; the catalog's routines and indexes;
//! - a debugger ([`debug`], version 9, draft): `HLINV/ASSETS` with its
//!   source and variables, stops in the connection job's `CALL` and in
//!   display sessions' jobs ([`System::interactive_job`],
//!   [`System::call_in`]);
//! - the ends a client must meet: the server ending ([`System::end`]:
//!   every connection gets `ServerEnding` and the [`Listener`] refuses
//!   connects until [`System::restart`]), an operator ending one job, a
//!   connection's socket dropped mid-request
//!   ([`System::drop_connection`]) and a panic in a connection's thread
//!   ([`System::crash`]).

pub mod commands;
pub mod debug;
pub mod develop;
pub mod files;
pub mod fixtures;
mod host;
pub mod jobs;
mod listen;
pub mod manage;
pub mod messages;
pub mod spool;
pub mod sql;

use std::collections::HashMap;
use std::io;
use std::net::{Shutdown, SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError, Weak};
use std::thread::{self, JoinHandle};
use std::time::Duration;

pub use host::{MemHost, MemThread};
pub use listen::Listener;

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
        drop(
            self.cv
                .wait_while(g, |o| !*o)
                .unwrap_or_else(PoisonError::into_inner),
        );
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
    /// Special authorities (`*JOBCTL`, ...).
    pub special: Vec<String>,
}

/// The system: what every connection shares.
pub struct System {
    /// Holds `system_info`.
    pub gate: Gate,
    /// An end for one connection job, the first whose poll takes it
    /// (an operator's `ENDJOB`); [`System::end`] sets it.
    pub ending: Mutex<Option<Ending>>,
    /// The server is ended (`ENDTCPSVR`) until [`System::restart`]:
    /// every connection's poll sees it, each sends `ServerEnding` and
    /// closes, and listeners refuse connects.
    pub server_ending: AtomicBool,
    /// Server ends so far; a connection opened before one ends with it,
    /// a restart after it or not.
    server_ends: AtomicU64,
    /// [`System::server_ends`] when each connection job started.
    opened_at: Mutex<HashMap<u32, u64>>,
    /// The listeners' ports.
    ports: Mutex<Vec<Weak<listen::Port>>>,
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
    /// Each connection job's current library, once `CHGCURLIB` set it:
    /// job state, its own in every job.
    pub curlib: Mutex<HashMap<u32, String>>,
    /// The output queues and spooled files.
    pub spool: Mutex<spool::Spool>,
    /// The message queues.
    pub messages: Mutex<messages::Queues>,
    /// A panic to come, once.
    pub crash: Mutex<Option<Crash>>,
    /// The database.
    pub sql: Mutex<sql::Db>,
    /// The integrated file system ([`files`]).
    pub files: files::Files,
    /// Subsystems, system values, PTFs, disks, hardware, the IPL
    /// ([`manage`]).
    pub manage: Mutex<manage::Manage>,
    /// Libraries, objects, source members, library lists ([`develop`]).
    pub develop: Mutex<develop::Develop>,
    /// Jobs and their logs ([`jobs`]).
    pub jobs: Mutex<jobs::Jobs>,
    /// The debugger's sessions, service entry points and other jobs.
    pub debug: debug::Debugs,
    /// The commands running, by job, while they run.
    running: Mutex<HashMap<u32, String>>,
    started: Condvar,
    /// The server's side of each connection's socket, by job.
    sockets: Mutex<HashMap<u32, TcpStream>>,
    tokens: AtomicU32,
}

/// A command running in a job, from [`System::command_runs`] until
/// dropped.
struct RunningGuard {
    sys: Arc<System>,
    job: u32,
}

impl Drop for RunningGuard {
    fn drop(&mut self) {
        lock(&self.sys.running).remove(&self.job);
    }
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
            server_ending: AtomicBool::new(false),
            server_ends: AtomicU64::new(0),
            opened_at: Mutex::new(HashMap::new()),
            ports: Mutex::new(Vec::new()),
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
            curlib: Mutex::new(HashMap::new()),
            spool: Mutex::new(spool::Spool::default()),
            messages: Mutex::new(messages::Queues::default()),
            crash: Mutex::new(None),
            sql: Mutex::new(sql::Db::default()),
            files: files::Files::new(),
            manage: Mutex::new(manage::Manage::shipped()),
            develop: Mutex::new(develop::Develop::shipped()),
            jobs: Mutex::new(jobs::Jobs::shipped()),
            debug: debug::Debugs::default(),
            running: Mutex::new(HashMap::new()),
            started: Condvar::new(),
            sockets: Mutex::new(HashMap::new()),
            tokens: AtomicU32::new(0),
        };
        s.gate.set(true);
        s.user("BOB", "SECRET1");
        s.user("WARN", "WARNPW1");
        s.edit("WARN", |u| u.expires = Some(EPOCH + 3 * 86_400));
        s.user("ANN", "ANNPW01");
        s.user("OPER", "OPERPW1");
        s.edit("OPER", |u| u.special = vec!["*JOBCTL".into()]);
        s.user("SECOFR", "SECPW01");
        s.edit("SECOFR", |u| {
            u.special = [
                "*ALLOBJ",
                "*SECADM",
                "*JOBCTL",
                "*AUDIT",
                "*IOSYSCFG",
                "*SERVICE",
            ]
            .map(String::from)
            .to_vec();
        });
        fixtures::install(&mut lock(&s.spool));
        messages::install(&mut lock(&s.messages));
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

    /// End connections from outside. [`Ending::Server`] is `ENDTCPSVR`:
    /// every listener closes its port, so connects are refused until
    /// [`System::restart`], and every open connection sends
    /// `ServerEnding` and closes. Any other end is one job's, taken by
    /// the first connection that polls.
    pub fn end(&self, how: Ending) {
        if how == Ending::Server {
            self.server_ending.store(true, Ordering::SeqCst);
            for p in self.live_ports() {
                p.close();
            }
            self.server_ends.fetch_add(1, Ordering::SeqCst);
        } else {
            *lock(&self.ending) = Some(how);
        }
    }

    /// Start the server again after [`System::end`] with
    /// [`Ending::Server`], as `STRTCPSVR` does: every listener opens its
    /// port again, at the same address.
    ///
    /// # Errors
    /// A port that cannot be bound again.
    pub fn restart(&self) -> io::Result<()> {
        self.server_ending.store(false, Ordering::SeqCst);
        for p in self.live_ports() {
            p.open()?;
        }
        Ok(())
    }

    /// Listen on a free loopback port, as `QAPD` does: a connection job
    /// for each connection, connects refused while the server is ended.
    ///
    /// # Errors
    /// Loopback cannot be bound.
    pub fn listen(self: &Arc<Self>, cfg: Config) -> io::Result<Listener> {
        Listener::start(self, cfg)
    }

    /// Whether connection job `job` must end with the server: it is
    /// ended now, or was since the job started.
    fn server_ended_for(&self, job: u32) -> bool {
        if self.server_ending.load(Ordering::SeqCst) {
            return true;
        }
        let at = lock(&self.opened_at).get(&job).copied().unwrap_or(0);
        self.server_ends.load(Ordering::SeqCst) > at
    }

    /// Connection job `job` starts now.
    fn opened(&self, job: u32) {
        lock(&self.opened_at).insert(job, self.server_ends.load(Ordering::SeqCst));
    }

    fn live_ports(&self) -> Vec<Arc<listen::Port>> {
        let mut ports = lock(&self.ports);
        ports.retain(|w| w.strong_count() > 0);
        ports.iter().filter_map(Weak::upgrade).collect()
    }

    /// Close connection job `job`'s socket on the server's side at
    /// once, as a network drop would: whatever is in progress gets no
    /// reply and no event, the client just sees the connection close.
    /// False when the job has no connection open.
    pub fn drop_connection(&self, job: u32) -> bool {
        let Some(s) = lock(&self.sockets).remove(&job) else {
            return false;
        };
        let _ = s.shutdown(Shutdown::Both);
        true
    }

    /// [`System::drop_connection`] of every connection open; how many.
    pub fn drop_all(&self) -> usize {
        let all: Vec<TcpStream> = lock(&self.sockets).drain().map(|(_, s)| s).collect();
        for s in &all {
            let _ = s.shutdown(Shutdown::Both);
        }
        all.len()
    }

    /// Wait until job `job` runs a command, at most `timeout`; the
    /// command's name, or `None` at the deadline. For tests that act
    /// while a command runs (a cancel) without a sleep.
    pub fn wait_running(&self, job: u32, timeout: Duration) -> Option<String> {
        let g = lock(&self.running);
        let (g, _) = self
            .started
            .wait_timeout_while(g, timeout, |r| !r.contains_key(&job))
            .unwrap_or_else(PoisonError::into_inner);
        g.get(&job).cloned()
    }

    /// Job `job` starts command `name`, until the guard goes.
    fn command_runs(self: &Arc<Self>, job: u32, name: &str) -> RunningGuard {
        lock(&self.running).insert(job, name.to_string());
        self.started.notify_all();
        RunningGuard {
            sys: Arc::clone(self),
            job,
        }
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
        let r = tickets::check(
            &mut lock(&self.tickets),
            &t,
            &user,
            self.now(),
            tls,
            allowed,
            profile_ok,
        );
        lock(&self.audit).push(format!("PS V {user}"));
        r.is_ok()
    }

    /// The 200 MiB file ([`files::BIG`]), made the first time a test
    /// asks; its path.
    pub fn big_file(&self) -> &'static str {
        self.files.big_file()
    }

    /// Put `m` on queue `lib/name` from outside, as another job sends
    /// it (its key filled in): for event tests. `None` when there is
    /// no such queue.
    pub fn inject_message(
        &self,
        lib: &str,
        name: &str,
        m: messages::Msg,
    ) -> Option<qap_proto::ops::messages::Key> {
        let mut qs = lock(&self.messages);
        let i = qs.find(lib, name)?;
        Some(messages::key(qs.queues[i].put(m)))
    }

    /// The job holding queue `lib/name` in break mode, if any.
    #[must_use]
    pub fn break_holder(&self, lib: &str, name: &str) -> Option<u32> {
        let qs = lock(&self.messages);
        qs.find(lib, name).and_then(|i| qs.queues[i].break_job)
    }

    /// Add spooled file `f` at the end of its queue from outside, as a
    /// job's printer file would: for event tests.
    pub fn spool_add(&self, f: spool::File) -> qap_proto::ops::spool::SpooledFileId {
        let id = f.id.clone();
        lock(&self.spool).file(f);
        id
    }

    /// Change spooled file `id` from outside (its status, attributes or
    /// queue): for event tests. False when there is no such file.
    pub fn spool_change(
        &self,
        id: &qap_proto::ops::spool::SpooledFileId,
        f: impl FnOnce(&mut spool::File),
    ) -> bool {
        lock(&self.spool).change(id, f)
    }

    /// Remove spooled file `id` from outside: for event tests. False
    /// when there is no such file.
    pub fn spool_remove(&self, id: &qap_proto::ops::spool::SpooledFileId) -> bool {
        lock(&self.spool).remove(id)
    }

    /// The other job holding [`files::LOCKED`] lets it go.
    pub fn release_lock(&self) {
        self.files.release_lock();
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
/// When loopback cannot be bound, or the server is ended (connects are
/// refused; see [`try_serve_loopback_as`]).
#[must_use]
pub fn serve_loopback(cfg: Config, system: &Arc<System>) -> (TcpStream, JoinHandle<Closed>) {
    serve_loopback_as(cfg, system, next_job())
}

/// [`serve_loopback`] as connection job `job`.
///
/// # Panics
/// As [`serve_loopback`].
#[must_use]
pub fn serve_loopback_as(
    cfg: Config,
    system: &Arc<System>,
    job: u32,
) -> (TcpStream, JoinHandle<Closed>) {
    try_serve_loopback_as(cfg, system, job).expect("loopback")
}

/// [`serve_loopback_as`], answering an error rather than panicking.
///
/// # Errors
/// `ConnectionRefused` while the server is ended, as a connect to the
/// real system's port is; loopback failures.
pub fn try_serve_loopback_as(
    cfg: Config,
    system: &Arc<System>,
    job: u32,
) -> io::Result<(TcpStream, JoinHandle<Closed>)> {
    if system.server_ending.load(Ordering::SeqCst) {
        return Err(io::Error::new(
            io::ErrorKind::ConnectionRefused,
            "the QAP server is ended",
        ));
    }
    let l = TcpListener::bind("127.0.0.1:0")?;
    let addr = l.local_addr()?;
    let sys = Arc::clone(system);
    let server = thread::spawn(move || match l.accept() {
        Ok((s, peer)) => serve_socket(cfg, &sys, job, s, peer),
        Err(e) => Closed::Broken(format!("accept: {e}")),
    });
    let s = TcpStream::connect(addr)?;
    Ok((s, server))
}

/// Serve one accepted connection as job `job` until it closes.
fn serve_socket(
    cfg: Config,
    sys: &Arc<System>,
    job: u32,
    s: TcpStream,
    peer: SocketAddr,
) -> Closed {
    if let Ok(c) = s.try_clone() {
        lock(&sys.sockets).insert(job, c);
    }
    // Not a read timeout: see `Polled`.
    let s = match Polled::new(s) {
        Ok(s) => s,
        Err(e) => return Closed::Broken(format!("non-blocking: {e}")),
    };
    let mut host = MemHost::new(Arc::clone(sys), job);
    let why = crate::serve(s, &mut host, cfg, identity(job, Some(peer)));
    lock(&sys.sockets).remove(&job);
    lock(&sys.opened_at).remove(&job);
    // The job ends: its locks and library list go.
    lock(&sys.develop).release(job);
    why
}
