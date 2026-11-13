// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `fsync()` off the caller's thread. The storage service answers every
//! job on one thread, and a sync can take milliseconds (under QEMU a
//! flush is the host's `fdatasync` of the disk image), so a request that
//! may wait (`crate::lease::defer`) hands the sync to the syncer thread
//! and waits for its ticket as a remote mount's request does
//! ([`crate::remote::request`]); its next run takes the result. The
//! thread takes every sync queued and makes one per file: a sync begun
//! after a write was made covers it, so syncs that queue while one runs
//! share the next. Outside such a request the sync is made in place.

use std::collections::HashMap;
use std::sync::{Condvar, LazyLock, Mutex, PoisonError};
use std::time::{Duration, Instant};

use crate::backend::{BFile, SyncHandle};
use crate::errno::{Errno, Result};
use crate::lease::defer::Deferral;
use crate::remote::request::{self, Found};
use crate::remote::tickets;
use crate::remote::types::RemoteError;
use crate::remote::work::Answer;

/// The mount number a local sync's wait names (no remote mount has it).
pub const LOCAL_SYNC: u32 = u32::MAX;

/// One sync asked for.
struct Job {
    h: SyncHandle,
    data_only: bool,
    token: u64,
    key: String,
    ticket: u64,
}

struct Queue {
    jobs: Mutex<(Vec<Job>, bool)>,
    cv: Condvar,
}

static QUEUE: LazyLock<Queue> = LazyLock::new(|| Queue {
    jobs: Mutex::new((Vec::new(), false)),
    cv: Condvar::new(),
});

/// Sync `file` for the request in progress: `None` when it must be done
/// here (no request that may wait, or a backend without handles), else
/// the result once the syncer made it, or `EAGAIN` with the request
/// deferred until then.
pub(crate) fn sync<F: BFile>(file: &F, data_only: bool) -> Option<Result<()>> {
    let t = request::current()?;
    if !request::allowed() {
        return None;
    }
    let key = request::occurrence("fsync");
    match request::slot(t, &key) {
        Found::Done(r) => return Some(r.map(|_| ()).map_err(RemoteError::errno)),
        Found::InFlight(k) => return Some(wait(t, k)),
        Found::None => {}
    }
    let h = file.sync_handle()?;
    let ticket = tickets::issue();
    request::queued(t, &key, ticket);
    push(Job {
        h,
        data_only,
        token: t.id,
        key,
        ticket,
    });
    Some(wait(t, ticket))
}

fn wait(t: request::Token, ticket: u64) -> Result<()> {
    request::note_waiting();
    crate::lease::defer::defer_remote(Deferral::Remote {
        fs: LOCAL_SYNC,
        ticket,
        token: t.id,
        until: Instant::now() + Duration::from_hours(24),
    });
    Err(Errno::EAGAIN)
}

fn push(j: Job) {
    let q = &*QUEUE;
    let mut g = q.jobs.lock().unwrap_or_else(PoisonError::into_inner);
    g.0.push(j);
    if !g.1 {
        g.1 = true;
        let started = std::thread::Builder::new()
            .name("qs-ifs-sync".into())
            .spawn(run)
            .is_ok();
        if !started {
            // No thread: the syncs are made here.
            g.1 = false;
            let jobs = std::mem::take(&mut g.0);
            drop(g);
            sync_batch(jobs);
            return;
        }
    }
    drop(g);
    q.cv.notify_one();
}

/// The syncer: every queued sync, one per file, until the process ends.
fn run() {
    let q = &*QUEUE;
    loop {
        let jobs = {
            let mut g = q.jobs.lock().unwrap_or_else(PoisonError::into_inner);
            while g.0.is_empty() {
                g = q.cv.wait(g).unwrap_or_else(PoisonError::into_inner);
            }
            std::mem::take(&mut g.0)
        };
        sync_batch(jobs);
    }
}

/// Sync each file of `jobs` once (data only when every job asked only
/// that) and answer every job.
fn sync_batch(jobs: Vec<Job>) {
    let mut by_file: HashMap<(u64, u64), Vec<Job>> = HashMap::new();
    for j in jobs {
        by_file.entry(j.h.key).or_default().push(j);
    }
    for (_, group) in by_file {
        let data_only = group.iter().all(|j| j.data_only);
        let r = (group[0].h.sync)(data_only);
        for j in group {
            let a = r.map(|()| Answer::Unit).map_err(RemoteError::Errno);
            request::post(j.token, &j.key, a);
            tickets::finish(j.ticket);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU32, Ordering};

    fn job(
        key: (u64, u64),
        data_only: bool,
        calls: &Arc<AtomicU32>,
        saw: &Arc<Mutex<Vec<bool>>>,
    ) -> Job {
        let c = Arc::clone(calls);
        let s = Arc::clone(saw);
        Job {
            h: SyncHandle {
                key,
                sync: Box::new(move |d| {
                    c.fetch_add(1, Ordering::Relaxed);
                    s.lock().unwrap_or_else(PoisonError::into_inner).push(d);
                    Ok(())
                }),
            },
            data_only,
            token: 0,
            key: String::new(),
            ticket: tickets::issue(),
        }
    }

    /// Syncs of one file queued together are one sync, a full one when
    /// any asked for it; every ticket is finished.
    #[test]
    fn one_sync_per_file() {
        let calls = Arc::new(AtomicU32::new(0));
        let saw = Arc::new(Mutex::new(Vec::new()));
        let jobs = vec![
            job((1, 7), true, &calls, &saw),
            job((1, 7), false, &calls, &saw),
            job((1, 7), true, &calls, &saw),
            job((1, 8), true, &calls, &saw),
        ];
        let tickets: Vec<u64> = jobs.iter().map(|j| j.ticket).collect();
        sync_batch(jobs);
        assert_eq!(calls.load(Ordering::Relaxed), 2);
        let mut s = saw.lock().unwrap_or_else(PoisonError::into_inner).clone();
        s.sort_unstable();
        assert_eq!(s, vec![false, true]);
        assert!(tickets.iter().all(|t| !tickets::pending(*t)));
    }
}
