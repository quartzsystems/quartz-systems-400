// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What a connection's close says: one `info` line naming the
//! connection, its job, why it closed and the requests still
//! outstanding. Its threads run under a watch that turns a panic into
//! the reason; under `panic = "abort"` the panic hook logs the close
//! itself, since nothing runs after it.

use std::any::Any;
use std::cell::RefCell;
use std::collections::BTreeSet;
use std::fmt;
use std::net::SocketAddr;
use std::panic::{self, AssertUnwindSafe, PanicHookInfo};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, Once};
use std::thread::{self, JoinHandle};

use qap_proto::types::JobId;

use crate::Closed;
use crate::conn::lock;

/// Who a connection is, for its close line.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Identity {
    /// The connection's number, unique in its server.
    pub connection: u64,
    /// The connection job.
    pub job: JobId,
    /// The client's address, when known.
    pub peer: Option<SocketAddr>,
}

/// One connection's close: who, what is outstanding, a panic seen.
pub(crate) struct Watch {
    who: Identity,
    /// Requests without their final reply written.
    pub outstanding: Mutex<BTreeSet<u32>>,
    /// The signed-on profile, kept for the close line.
    pub profile: Mutex<Option<String>>,
    panic: Mutex<Option<String>>,
    logged: AtomicBool,
}

/// Connections open in this process.
static LIVE: Mutex<Vec<Arc<Watch>>> = Mutex::new(Vec::new());

thread_local! {
    /// The connection this thread works for.
    static CURRENT: RefCell<Option<Arc<Watch>>> = const { RefCell::new(None) };
}

impl Watch {
    /// A connection opens.
    pub fn open(who: Identity) -> Arc<Self> {
        hook();
        let w = Arc::new(Self {
            who,
            outstanding: Mutex::new(BTreeSet::new()),
            profile: Mutex::new(None),
            panic: Mutex::new(None),
            logged: AtomicBool::new(false),
        });
        lock(&LIVE).push(Arc::clone(&w));
        w
    }

    /// This thread works for the connection until the guard drops.
    pub fn enter(self: &Arc<Self>) -> Entered {
        let before = CURRENT.with(|c| c.replace(Some(Arc::clone(self))));
        Entered(before)
    }

    /// Log the close, once; the connection is no longer live.
    pub fn closed(&self, why: &Closed) {
        if self.logged.swap(true, Ordering::SeqCst) {
            return;
        }
        lock(&LIVE).retain(|w| !std::ptr::eq(Arc::as_ptr(w), self));
        let ids: Vec<u32> = lock(&self.outstanding).iter().copied().collect();
        let profile = lock(&self.profile).clone().unwrap_or_default();
        tracing::info!(
            target: "qapd",
            connection = self.who.connection,
            job = %Job(&self.who.job),
            peer = %Peer(self.who.peer),
            profile = %profile,
            reason = %why,
            outstanding = ?ids,
            "QAP connection closed"
        );
    }

    /// The reason a thread's panic gives: the hook's text, with where,
    /// or the payload's.
    pub fn panicked(&self, payload: &(dyn Any + Send)) -> Closed {
        let seen = lock(&self.panic).clone();
        Closed::Panic(seen.unwrap_or_else(|| text(thread::current().name(), payload_str(payload), None)))
    }

    fn note(&self, t: &str) {
        let mut p = lock(&self.panic);
        if p.is_none() {
            *p = Some(t.to_string());
        }
    }
}

/// Puts a thread's previous connection back.
pub(crate) struct Entered(Option<Arc<Watch>>);

impl Drop for Entered {
    fn drop(&mut self) {
        let before = self.0.take();
        let _ = CURRENT.try_with(|c| c.replace(before));
    }
}

/// Start thread `name` for connection `w`; a panic in `body` goes to
/// `report` as the reason to close.
pub(crate) fn spawn(
    name: String,
    w: &Arc<Watch>,
    body: impl FnOnce() + Send + 'static,
    report: impl FnOnce(Closed) + Send + 'static,
) -> JoinHandle<()> {
    let w = Arc::clone(w);
    thread::Builder::new()
        .name(name)
        .spawn(move || {
            let _in = w.enter();
            if let Err(p) = panic::catch_unwind(AssertUnwindSafe(body)) {
                report(w.panicked(&*p));
            }
        })
        .expect("a connection thread")
}

/// The process is ending under its connections (a lost link,
/// `ENDJOB *IMMED`): log each one's close with `why` now.
pub fn exiting(why: &Closed) {
    let live: Vec<Arc<Watch>> = lock(&LIVE).clone();
    for w in live {
        w.closed(why);
    }
}

/// The panic hook, once per process, ahead of the one before it.
fn hook() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let before = panic::take_hook();
        panic::set_hook(Box::new(move |info| {
            seen(info);
            before(info);
        }));
    });
}

/// A panic on one of a connection's threads: noted for the reason, or
/// logged as the close when the process aborts next.
fn seen(info: &PanicHookInfo<'_>) {
    let Ok(Some(w)) = CURRENT.try_with(|c| c.try_borrow().ok().and_then(|c| c.clone())) else {
        return;
    };
    let msg = info.payload_as_str().unwrap_or("(no message)");
    let t = text(thread::current().name(), msg, info.location().map(ToString::to_string));
    w.note(&t);
    if cfg!(panic = "abort") {
        w.closed(&Closed::Panic(t));
    }
}

fn payload_str(p: &(dyn Any + Send)) -> &str {
    p.downcast_ref::<&str>()
        .copied()
        .or_else(|| p.downcast_ref::<String>().map(String::as_str))
        .unwrap_or("(no message)")
}

fn text(thread: Option<&str>, msg: &str, at: Option<String>) -> String {
    let thread = thread.unwrap_or("unnamed");
    match at {
        Some(at) => format!("thread {thread}: {msg} at {at}"),
        None => format!("thread {thread}: {msg}"),
    }
}

/// A job as IBM writes it: `number/user/name`.
struct Job<'a>(&'a JobId);

impl fmt::Display for Job<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}/{}", self.0.number, self.0.user, self.0.name)
    }
}

/// The client's address, `-` when unknown.
struct Peer(Option<SocketAddr>);

impl fmt::Display for Peer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            Some(a) => a.fmt(f),
            None => f.write_str("-"),
        }
    }
}
