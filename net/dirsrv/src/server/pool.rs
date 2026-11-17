// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The connection threads: a pool of workers, each with its own link to
//! the service made once and kept across the connections it serves, so
//! a new connection costs neither a thread nor a link. A connection goes
//! to an idle worker when there is one, else to a new worker; workers
//! beyond [`KEEP`] that stay idle for [`IDLE_EXIT`] end. A worker whose
//! link broke makes a new one for its next connection. Identity is the
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
/// How long a worker beyond [`KEEP`] waits for a connection before it ends.
pub const IDLE_EXIT: Duration = Duration::from_secs(60);
/// How often a waiting worker looks at the end.
const TICK: Duration = Duration::from_millis(200);

/// A connection and what it is given.
pub type Work = (TcpStream, Setup);

/// The workers.
pub struct Pool<L: Links> {
    links: Arc<L>,
    stop: Arc<AtomicBool>,
    /// Workers waiting for a connection, less the connections queued
    /// for them.
    idle: Arc<AtomicUsize>,
    tx: mpsc::Sender<Work>,
    rx: Arc<Mutex<mpsc::Receiver<Work>>>,
    threads: Vec<JoinHandle<()>>,
}

impl<L: Links> Pool<L> {
    /// A pool with [`KEEP`] workers ready.
    pub fn new(links: Arc<L>, stop: Arc<AtomicBool>) -> Self {
        let (tx, rx) = mpsc::channel();
        let mut p = Self {
            links,
            stop,
            idle: Arc::new(AtomicUsize::new(0)),
            tx,
            rx: Arc::new(Mutex::new(rx)),
            threads: Vec::new(),
        };
        for _ in 0..KEEP {
            p.idle.fetch_add(1, Ordering::SeqCst);
            if !p.spawn(None) {
                p.idle.fetch_sub(1, Ordering::SeqCst);
            }
        }
        p
    }

    /// Serve `w` on an idle worker, or a new one.
    pub fn serve(&mut self, w: Work) {
        self.threads.retain(|t| !t.is_finished());
        let reserved = self
            .idle
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |n| n.checked_sub(1))
            .is_ok();
        if reserved {
            if let Err(mpsc::SendError(w)) = self.tx.send(w) {
                refuse(w.0);
            }
            return;
        }
        let first = Arc::new(Mutex::new(Some(w)));
        if !self.spawn(Some(Arc::clone(&first))) {
            let w = first.lock().unwrap_or_else(PoisonError::into_inner).take();
            if let Some(w) = w {
                tracing::warn!("LDAP: no thread for a connection");
                refuse(w.0);
            }
        }
    }

    /// Start a worker, serving `first` before it waits; whether it started.
    fn spawn(&mut self, first: Option<Arc<Mutex<Option<Work>>>>) -> bool {
        let links = Arc::clone(&self.links);
        let rx = Arc::clone(&self.rx);
        let idle = Arc::clone(&self.idle);
        let stop = Arc::clone(&self.stop);
        let t = std::thread::Builder::new()
            .name("ldap-conn".into())
            .spawn(move || {
                let first =
                    first.and_then(|f| f.lock().unwrap_or_else(PoisonError::into_inner).take());
                worker(&*links, first, &rx, &idle, &stop);
            });
        match t {
            Ok(t) => {
                self.threads.push(t);
                true
            }
            Err(e) => {
                tracing::warn!("LDAP: no thread: {e}");
                false
            }
        }
    }

    /// Wait for every worker (the stop flag is set).
    pub fn join(self) {
        drop(self.tx);
        for t in self.threads {
            let _ = t.join();
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

fn worker<L: Links>(
    links: &L,
    first: Option<Work>,
    rx: &Mutex<mpsc::Receiver<Work>>,
    idle: &AtomicUsize,
    stop: &AtomicBool,
) {
    // A worker made ahead opens its link at once; one made for a
    // connection when it serves it.
    let mut svc = if first.is_none() {
        links.link().ok()
    } else {
        None
    };
    let mut next = first;
    loop {
        let w = match next.take() {
            Some(w) => w,
            None => match wait(rx, idle, stop) {
                Some(w) => w,
                None => return,
            },
        };
        let (s, setup) = w;
        let _ = s.set_nonblocking(false);
        let link = match svc.take() {
            Some(l) => Ok(l),
            None => links.link(),
        };
        match link {
            Ok(l) => {
                let (end, l) = Connection::serve_with(l, s, setup);
                tracing::debug!("LDAP connection ended: {end:?}");
                if end != End::Broken {
                    svc = Some(l);
                }
            }
            Err(e) => {
                tracing::warn!("LDAP: no link to the storage service: {e}");
                refuse(s);
            }
        }
        if stop.load(Ordering::Relaxed) {
            return;
        }
        idle.fetch_add(1, Ordering::SeqCst);
    }
}

/// The next connection for an idle worker (counted in `idle`); `None`
/// when it is to end, having taken itself out of the count.
fn wait(rx: &Mutex<mpsc::Receiver<Work>>, idle: &AtomicUsize, stop: &AtomicBool) -> Option<Work> {
    let since = Instant::now();
    loop {
        if stop.load(Ordering::Relaxed) {
            return None;
        }
        let got = rx
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .recv_timeout(TICK);
        match got {
            Ok(w) => return Some(w),
            Err(mpsc::RecvTimeoutError::Disconnected) => return None,
            Err(mpsc::RecvTimeoutError::Timeout) => {}
        }
        if since.elapsed() >= IDLE_EXIT {
            let left = idle.fetch_update(Ordering::SeqCst, Ordering::SeqCst, |n| {
                (n > KEEP).then(|| n - 1)
            });
            if left.is_ok() {
                return None;
            }
        }
    }
}
