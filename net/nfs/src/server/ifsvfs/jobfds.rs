// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The kept descriptors of every thread of one daemon job, so a thread
//! can close another's idle ones. A kept descriptor is an open like any
//! other: one held for writing by a thread whose connection idles would
//! refuse an open that denies writing (an NLM `SHARE`, an `NFSv4`
//! `OPEN` with a deny mode) for as long as it idles. Descriptors are
//! the job's, so any thread may close them; a thread inside a call is
//! left alone (its cache is marked busy under its own lock, which is
//! where another thread looks). The threads of one job are those of one
//! server start (one [`Instance`]).

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError, Weak};
use std::time::Instant;

use ifs::ops::audit::FileId;

use super::fdcache::FdCache;
use crate::server::instance::Instance;

/// One thread's kept descriptors.
#[derive(Debug)]
pub(crate) struct Member {
    cache: Mutex<FdCache>,
    /// Calls of the thread in progress (nested calls count).
    busy: AtomicUsize,
}

/// A call in progress on a member: the member is busy until it drops.
pub(crate) struct Busy<'a>(&'a Member);

impl Drop for Busy<'_> {
    fn drop(&mut self) {
        self.0.busy.fetch_sub(1, Ordering::SeqCst);
    }
}

impl Member {
    /// The cache, locked.
    pub(crate) fn cache(&self) -> MutexGuard<'_, FdCache> {
        self.cache.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Marks a call in progress (under the cache's lock, so another
    /// thread that took the lock first finishes with the cache before).
    pub(crate) fn enter(&self) -> Busy<'_> {
        let _g = self.cache();
        self.busy.fetch_add(1, Ordering::SeqCst);
        Busy(self)
    }

    /// When the thread is not in a call: what `f` takes out of its cache.
    fn take_idle(&self, f: impl FnOnce(&mut FdCache) -> Vec<i32>) -> Vec<i32> {
        let mut c = self.cache();
        if self.busy.load(Ordering::SeqCst) != 0 {
            return Vec::new();
        }
        f(&mut c)
    }
}

type Job = (Weak<Instance>, Vec<Weak<Member>>);

static JOBS: Mutex<Vec<Job>> = Mutex::new(Vec::new());

/// A member for a thread of the job of `instance`.
pub(crate) fn join(instance: &Arc<Instance>, cache: FdCache) -> Arc<Member> {
    let m = Arc::new(Member {
        cache: Mutex::new(cache),
        busy: AtomicUsize::new(0),
    });
    let mut jobs = JOBS.lock().unwrap_or_else(PoisonError::into_inner);
    jobs.retain_mut(|(i, ms)| {
        ms.retain(|w| w.strong_count() > 0);
        i.strong_count() > 0 && !ms.is_empty()
    });
    let w = Arc::downgrade(&m);
    match jobs
        .iter_mut()
        .find(|(i, _)| i.upgrade().is_some_and(|i| Arc::ptr_eq(&i, instance)))
    {
        Some((_, ms)) => ms.push(w),
        None => jobs.push((Arc::downgrade(instance), vec![w])),
    }
    m
}

/// The other threads of `me`'s job.
fn others(instance: &Arc<Instance>, me: &Arc<Member>) -> Vec<Arc<Member>> {
    JOBS.lock()
        .unwrap_or_else(PoisonError::into_inner)
        .iter()
        .filter(|(i, _)| i.upgrade().is_some_and(|i| Arc::ptr_eq(&i, instance)))
        .flat_map(|(_, ms)| ms.iter().filter_map(Weak::upgrade))
        .filter(|m| !Arc::ptr_eq(m, me))
        .collect()
}

/// The idle threads' descriptors of `fid`: to close.
pub(crate) fn evict_object(instance: &Arc<Instance>, me: &Arc<Member>, fid: FileId) -> Vec<i32> {
    others(instance, me)
        .iter()
        .flat_map(|m| m.take_idle(|c| c.drop_object(fid)))
        .collect()
}

/// The idle threads' descriptors unused too long: to close.
pub(crate) fn expire_idle(instance: &Arc<Instance>, me: &Arc<Member>, now: Instant) -> Vec<i32> {
    others(instance, me)
        .iter()
        .flat_map(|m| m.take_idle(|c| c.expire(now)))
        .collect()
}
