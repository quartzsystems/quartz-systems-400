// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The connection threads: a pool of workers, each with its own link to
//! the service made once and kept across the connections it serves, so
//! a new connection costs neither a thread nor a link. Connections go to
//! one queue the idle workers take from; a new worker starts only when a
//! connection has waited [`SPAWN_AFTER`] with none free, up to [`MAX`]
//! (beyond it a connection waits for a worker to finish, so a burst of
//! connections cannot use up the job's or the service's descriptors).
//! Idle workers cost nothing: one waits on the queue, the others on its
//! lock, and none wakes on a timer but to end. Workers beyond [`KEEP`]
//! that stay idle for [`IDLE_EXIT`] end: each holds a link the service
//! polls, and many of them slowed every request. A worker whose link
//! broke makes a new one for its next connection. Identity is the
//! session's (`open` per connection, closed at its end), never the
//! link's.

use std::io::Write as _;
use std::net::TcpStream;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError, mpsc};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use ldap::ResultCode;

use super::conn::{Connection, End, Setup};
use super::{Links, reply};

/// Workers kept idle (and made at the start, their links opened).
pub const KEEP: usize = 4;
/// The most workers, so the most connections served at once.
pub const MAX: usize = 128;
/// How long a worker beyond [`KEEP`] waits for a connection before it ends.
pub const IDLE_EXIT: Duration = Duration::from_secs(10);
/// How long a connection waits for a free worker before one starts.
pub const SPAWN_AFTER: Duration = Duration::from_millis(2);

/// A connection and what it is given.
pub type Work = (TcpStream, Setup);

/// What the workers share.
struct Shared {
    rx: Mutex<mpsc::Receiver<Work>>,
    /// Workers waiting for a connection.
    idle: AtomicUsize,
    /// Connections queued and not yet taken.
    pending: AtomicUsize,
    /// Workers running.
    workers: AtomicUsize,
    stop: Arc<AtomicBool>,
}

/// The workers.
pub struct Pool<L: Links> {
    links: Arc<L>,
    shared: Arc<Shared>,
    tx: mpsc::Sender<Work>,
    threads: Vec<JoinHandle<()>>,
    /// Since when connections have waited with no worker free.
    short: Option<Instant>,
}

impl<L: Links> Pool<L> {
    /// A pool with [`KEEP`] workers ready.
    pub fn new(links: Arc<L>, stop: Arc<AtomicBool>) -> Self {
        let (tx, rx) = mpsc::channel();
        let mut p = Self {
            links,
            shared: Arc::new(Shared {
                rx: Mutex::new(rx),
                idle: AtomicUsize::new(0),
                pending: AtomicUsize::new(0),
                workers: AtomicUsize::new(0),
                stop,
            }),
            tx,
            threads: Vec::new(),
            short: None,
        };
        for _ in 0..KEEP {
            p.spawn();
        }
        p
    }

    /// Workers running now.
    #[must_use]
    pub fn workers(&self) -> usize {
        self.shared.workers.load(Ordering::SeqCst)
    }

    /// Queue `w` for a worker.
    pub fn serve(&mut self, w: Work) {
        self.threads.retain(|t| !t.is_finished());
        let s = &self.shared;
        s.pending.fetch_add(1, Ordering::SeqCst);
        if let Err(mpsc::SendError(w)) = self.tx.send(w) {
            s.pending.fetch_sub(1, Ordering::SeqCst);
            refuse(w.0);
        }
        self.grow();
    }

    /// Whether connections wait with no worker free: the accept loop
    /// then calls [`Self::grow`] often.
    #[must_use]
    pub fn short(&self) -> bool {
        self.short.is_some()
    }

    /// Start a worker when connections have waited [`SPAWN_AFTER`] with
    /// none free.
    pub fn grow(&mut self) {
        let s = &self.shared;
        if s.pending.load(Ordering::SeqCst) <= s.idle.load(Ordering::SeqCst) {
            self.short = None;
            return;
        }
        let since = *self.short.get_or_insert_with(Instant::now);
        if since.elapsed() >= SPAWN_AFTER && s.workers.load(Ordering::SeqCst) < MAX {
            self.spawn();
            self.short = None;
        }
    }

    /// Start a worker.
    fn spawn(&mut self) {
        let links = Arc::clone(&self.links);
        let shared = Arc::clone(&self.shared);
        shared.workers.fetch_add(1, Ordering::SeqCst);
        let t = std::thread::Builder::new()
            .name("ldap-conn".into())
            .spawn(move || {
                worker(&*links, &shared);
                shared.workers.fetch_sub(1, Ordering::SeqCst);
            });
        match t {
            Ok(t) => self.threads.push(t),
            Err(e) => {
                self.shared.workers.fetch_sub(1, Ordering::SeqCst);
                tracing::warn!("LDAP: no thread: {e}");
            }
        }
    }

    /// End: the queue goes, so the waiting workers end; wait for every
    /// worker (the stop flag is set); connections still queued get the
    /// notice.
    pub fn join(self) {
        drop(self.tx);
        for t in self.threads {
            let _ = t.join();
        }
        let rx = self.shared.rx.lock().unwrap_or_else(PoisonError::into_inner);
        while let Ok((s, _)) = rx.try_recv() {
            refuse(s);
        }
    }
}

/// A notice that the server cannot take the connection.
fn refuse(mut s: TcpStream) {
    let _ = s.set_nonblocking(false);
    let _ = s.write_all(&reply::notice(
        ResultCode::Unavailable,
        "The directory server cannot reach its data.",
    ));
}

fn worker<L: Links>(links: &L, s: &Shared) {
    let mut svc = links.link().ok();
    while let Some((stream, setup)) = wait(s) {
        let _ = stream.set_nonblocking(false);
        let link = match svc.take() {
            Some(l) => Ok(l),
            None => links.link(),
        };
        match link {
            Ok(l) => {
                let (end, l) = Connection::serve_with(l, stream, setup);
                tracing::debug!("LDAP connection ended: {end:?}");
                if end != End::Broken {
                    svc = Some(l);
                }
            }
            Err(e) => {
                tracing::warn!("LDAP: no link to the storage service: {e}");
                refuse(stream);
            }
        }
    }
}

/// The next connection; `None` when the worker is to end: the queue is
/// gone (the job ends), or it stayed idle past [`IDLE_EXIT`] with more
/// than [`KEEP`] idle.
fn wait(s: &Shared) -> Option<Work> {
    s.idle.fetch_add(1, Ordering::SeqCst);
    let got = loop {
        if s.stop.load(Ordering::Relaxed) {
            break None;
        }
        let r = s
            .rx
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .recv_timeout(IDLE_EXIT);
        match r {
            Ok(w) => {
                s.pending.fetch_sub(1, Ordering::SeqCst);
                break Some(w);
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => break None,
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if s.idle.load(Ordering::SeqCst) > KEEP {
                    break None;
                }
            }
        }
    };
    s.idle.fetch_sub(1, Ordering::SeqCst);
    got
}
