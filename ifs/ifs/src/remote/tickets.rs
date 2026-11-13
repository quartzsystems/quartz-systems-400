// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Tickets: one per piece of queued remote work. A deferred operation
//! names the ticket it waits for; the ticket is pending until a worker
//! finishes the work. Finishing wakes whoever waits: threads blocked in
//! [`wait`] and the storage service's loop through the waker it set
//! ([`set_waker`]).

use std::collections::HashSet;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, LazyLock, Mutex, PoisonError};
use std::time::{Duration, Instant};

/// What the service's loop is woken by.
pub type Waker = Arc<dyn Fn() + Send + Sync>;

static NEXT: AtomicU64 = AtomicU64::new(1);
static PENDING: LazyLock<(Mutex<HashSet<u64>>, Condvar)> =
    LazyLock::new(|| (Mutex::new(HashSet::new()), Condvar::new()));
static WAKER: Mutex<Option<Waker>> = Mutex::new(None);

/// A new pending ticket.
pub fn issue() -> u64 {
    let t = NEXT.fetch_add(1, Ordering::Relaxed);
    PENDING
        .0
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .insert(t);
    t
}

/// The work of `t` is done (or dropped).
pub fn finish(t: u64) {
    let (set, cv) = &*PENDING;
    set.lock()
        .unwrap_or_else(PoisonError::into_inner)
        .remove(&t);
    cv.notify_all();
    wake();
}

/// Wake the waiters without finishing anything (an interrupt).
pub fn wake() {
    PENDING.1.notify_all();
    let w = WAKER.lock().unwrap_or_else(PoisonError::into_inner).clone();
    if let Some(w) = w {
        w();
    }
}

/// Whether `t`'s work is still to be done.
#[must_use]
pub fn pending(t: u64) -> bool {
    PENDING
        .0
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .contains(&t)
}

/// Block until `t` is done or `stop` says so (true), or `until` passes
/// (false). For callers that may block: tests and tools, never the
/// storage service's loop.
pub fn wait(t: u64, until: Instant, stop: &dyn Fn() -> bool) -> bool {
    let (set, cv) = &*PENDING;
    loop {
        // `stop` may take other locks: never under this one.
        if !pending(t) || stop() {
            return true;
        }
        let now = Instant::now();
        if now >= until {
            return false;
        }
        let left = (until - now).min(Duration::from_millis(20));
        let g = set.lock().unwrap_or_else(PoisonError::into_inner);
        if !g.contains(&t) {
            continue;
        }
        drop(
            cv.wait_timeout(g, left)
                .unwrap_or_else(PoisonError::into_inner),
        );
    }
}

/// Set what wakes the storage service's loop when work finishes.
pub fn set_waker(w: Option<Waker>) {
    *WAKER.lock().unwrap_or_else(PoisonError::into_inner) = w;
}
