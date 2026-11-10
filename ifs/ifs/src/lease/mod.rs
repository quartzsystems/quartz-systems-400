// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Leases across owners (`docs/NETSERVER.md` section 6). Each `NetServer`
//! job grants SMB leases in its own process; this table, one per
//! [`crate::ops::Ifs`], is how every other owner's operation reaches
//! them. A holder reserves a lease before its library grants it
//! ([`Leases::grant`], capped by what other owners hold and have open),
//! reports the leases it holds on a file after each change
//! ([`Leases::report`]), and polls for break orders ([`Leases::orders`]).
//! An open, write, truncation, byte-range lock, rename or removal by
//! another owner ([`Leases::gate`]) orders the conflicting holders to
//! break and, where SMB has it wait, defers until they report the lower
//! state or [`BREAK_TIMEOUT`] passes ([`defer`]). An owner's leases end
//! with it ([`Leases::release_owner`]).

pub mod defer;
pub mod state;

#[cfg(test)]
mod tests;

use std::collections::HashMap;
use std::sync::{Condvar, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use crate::errno::{Errno, Result};
use crate::lock::{ObjKey, Owner};
use defer::Deferral;
use state::{Trigger, W, break_to, candidates, needs_ack, requested};

/// How long a holder has to acknowledge a break (the SMB library's,
/// as Windows servers wait).
pub const BREAK_TIMEOUT: Duration = Duration::from_secs(35);

/// Time past the timeout before a waiter forces the break.
const SLACK: Duration = Duration::from_secs(1);

/// A lease's identity: the client GUID and the lease key.
pub type LeaseKey = [u8; 32];

/// One break order: break the leases of the file the holder knows as
/// `tag` to at most `keep`.
pub type Order = (u64, u8);

/// A lease another owner holds.
#[derive(Debug)]
struct Held {
    owner: Owner,
    /// What the holder calls the file.
    tag: u64,
    key: LeaseKey,
    state: u8,
    breaking: Option<Breaking>,
}

/// A break waiting for the holder's acknowledgment.
#[derive(Debug, Clone, Copy)]
struct Breaking {
    to: u8,
    deadline: Instant,
}

#[derive(Debug, Default)]
struct Inner {
    files: HashMap<ObjKey, Vec<Held>>,
    tags: HashMap<(Owner, u64), ObjKey>,
    orders: HashMap<Owner, Vec<Order>>,
}

/// Every owner's leases on the file system's objects.
#[derive(Debug, Default)]
pub struct Leases {
    inner: Mutex<Inner>,
    changed: Condvar,
}

/// What other owners have on an object, as a grant sees it.
#[derive(Debug, Clone, Copy, Default)]
pub struct Others {
    /// Another owner has it open.
    pub open: bool,
    /// Another owner holds a byte-range lock on it.
    pub locked: bool,
}

impl Leases {
    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Whether no lease is held.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.lock().files.is_empty()
    }

    /// `owner` is about to do `trigger` on `key`: order every other
    /// owner's conflicting lease broken. `EAGAIN` when the operation has
    /// to wait for an acknowledgment and its caller allowed it to defer
    /// ([`defer::deferring`]); otherwise it goes on.
    ///
    /// # Errors
    ///
    /// `EAGAIN` as above.
    pub fn gate(&self, owner: Owner, key: ObjKey, trigger: Trigger) -> Result<()> {
        let mut g = self.lock();
        let Inner { files, orders, .. } = &mut *g;
        let Some(list) = files.get_mut(&key) else {
            return Ok(());
        };
        let now = Instant::now();
        let mut until: Option<Instant> = None;
        let mut sent = false;
        for h in list.iter_mut().filter(|h| h.owner != owner) {
            let Some((to, wait)) = break_to(trigger, h.state) else {
                continue;
            };
            let keep = match &mut h.breaking {
                Some(b) if b.to & to == b.to => None,
                Some(b) => {
                    b.to &= to;
                    Some(b.to)
                }
                None if needs_ack(h.state) => {
                    h.breaking = Some(Breaking {
                        to,
                        deadline: now + BREAK_TIMEOUT,
                    });
                    Some(to)
                }
                None => {
                    h.state = to;
                    Some(to)
                }
            };
            if let Some(keep) = keep {
                order(orders, h.owner, h.tag, keep);
                sent = true;
            }
            if wait && let Some(b) = h.breaking {
                until = Some(until.map_or(b.deadline, |u| u.max(b.deadline)));
            }
        }
        drop(g);
        if sent {
            self.changed.notify_all();
        }
        match until {
            Some(until) if defer::defer(Deferral::Breaks {
                key,
                owner,
                trigger,
                until: until + SLACK,
            }) =>
            {
                Err(Errno::EAGAIN)
            }
            _ => Ok(()),
        }
    }

    /// `owner` asks for a lease `key` in state `want` on `obj`, which it
    /// calls `tag`: the best state other owners allow, recorded before
    /// the holder grants it. Write caching needs no other owner to have
    /// the object open or hold a lease on it; read and handle caching
    /// need no other owner's write caching or byte-range lock.
    pub fn grant(
        &self,
        owner: Owner,
        obj: ObjKey,
        tag: u64,
        key: LeaseKey,
        want: u8,
        others: Others,
    ) -> u8 {
        let want = requested(want);
        let mut g = self.lock();
        let Inner { files, tags, .. } = &mut *g;
        let list = files.entry(obj).or_default();
        let theirs: Vec<u8> = list
            .iter()
            .filter(|h| h.owner != owner)
            .map(|h| h.state)
            .collect();
        let rh = !others.locked && theirs.iter().all(|s| s & W == 0);
        let w = rh && !others.open && theirs.iter().all(|s| *s == 0);
        let granted = candidates(want)
            .find(|c| if c & W != 0 { w } else { rh })
            .unwrap_or(0);
        match list.iter_mut().find(|h| h.owner == owner && h.key == key) {
            Some(h) if h.breaking.is_none() => h.state |= granted,
            None if granted != 0 => list.push(Held {
                owner,
                tag,
                key,
                state: granted,
                breaking: None,
            }),
            _ => {}
        }
        if list.is_empty() {
            files.remove(&obj);
        } else {
            tags.insert((owner, tag), obj);
        }
        granted
    }

    /// `owner` holds `leases` (and no other) on the file it calls `tag`.
    /// A break completes once the lease is within what it was ordered to
    /// keep. A file `owner` never reserved a lease on is ignored.
    pub fn report(&self, owner: Owner, tag: u64, leases: &[(LeaseKey, u8)]) {
        let mut g = self.lock();
        let Inner { files, tags, .. } = &mut *g;
        let Some(obj) = tags.get(&(owner, tag)).copied() else {
            return;
        };
        let list = files.entry(obj).or_default();
        let state_of = |k: &LeaseKey| {
            leases
                .iter()
                .filter(|(lk, _)| lk == k)
                .fold(0, |s, (_, b)| s | (b & state::RWH))
        };
        list.retain(|h| h.owner != owner || h.tag != tag || state_of(&h.key) != 0);
        for (k, _) in leases {
            let s = state_of(k);
            if s == 0 {
                continue;
            }
            match list.iter_mut().find(|h| h.owner == owner && h.key == *k) {
                Some(h) => {
                    h.state = s;
                    if h.breaking.is_some_and(|b| s & !b.to == 0) {
                        h.breaking = None;
                    }
                }
                None => list.push(Held {
                    owner,
                    tag,
                    key: *k,
                    state: s,
                    breaking: None,
                }),
            }
        }
        if !list.iter().any(|h| h.owner == owner && h.tag == tag) {
            tags.remove(&(owner, tag));
        }
        if list.is_empty() {
            files.remove(&obj);
        }
        drop(g);
        self.changed.notify_all();
    }

    /// `owner`'s break orders. With none waiting and `wait` not zero,
    /// `EAGAIN` with a deferral when the caller allows one, so the poll
    /// waits for orders up to `wait`.
    ///
    /// # Errors
    ///
    /// `EAGAIN` as above.
    pub fn orders(&self, owner: Owner, wait: Duration) -> Result<Vec<Order>> {
        let mut g = self.lock();
        if let Some(v) = g.orders.remove(&owner) {
            return Ok(v);
        }
        drop(g);
        if !wait.is_zero()
            && defer::defer(Deferral::Orders {
                owner,
                until: Instant::now() + wait,
            })
        {
            return Err(Errno::EAGAIN);
        }
        Ok(Vec::new())
    }

    /// `owner` ended: its leases and orders go, and what waited on them
    /// goes on.
    pub fn release_owner(&self, owner: Owner) {
        let mut g = self.lock();
        g.files.retain(|_, list| {
            list.retain(|h| h.owner != owner);
            !list.is_empty()
        });
        g.tags.retain(|(o, _), _| *o != owner);
        g.orders.remove(&owner);
        drop(g);
        self.changed.notify_all();
    }

    /// Whether what `d` waits for has happened.
    #[must_use]
    pub fn ready(&self, d: &Deferral) -> bool {
        ready(&self.lock(), d)
    }

    /// Breaks whose holders did not acknowledge by their deadline count
    /// as done (the holder's library drops them itself).
    pub fn expire(&self, now: Instant) {
        let mut g = self.lock();
        let mut any = false;
        for h in g.files.values_mut().flatten() {
            if let Some(b) = h.breaking
                && b.deadline <= now
            {
                h.state = b.to;
                h.breaking = None;
                any = true;
            }
        }
        drop(g);
        if any {
            self.changed.notify_all();
        }
    }

    /// Block until `d` is ready (true) or its time is up (false), forcing
    /// breaks that timed out.
    pub fn wait(&self, d: &Deferral) -> bool {
        let mut g = self.lock();
        loop {
            if ready(&g, d) {
                return true;
            }
            let now = Instant::now();
            if now >= d.until() {
                drop(g);
                self.expire(now);
                return self.ready(d);
            }
            let left = (d.until() - now).min(Duration::from_secs(1));
            g = self
                .changed
                .wait_timeout(g, left)
                .unwrap_or_else(PoisonError::into_inner)
                .0;
        }
    }

    /// The leases on `obj`: owner, key and state, for displays and tests.
    #[must_use]
    pub fn on(&self, obj: ObjKey) -> Vec<(Owner, LeaseKey, u8)> {
        self.lock()
            .files
            .get(&obj)
            .map(|l| l.iter().map(|h| (h.owner, h.key, h.state)).collect())
            .unwrap_or_default()
    }
}

/// Queue `keep` for the holder `owner` of `tag`, lowering one queued.
fn order(orders: &mut HashMap<Owner, Vec<Order>>, owner: Owner, tag: u64, keep: u8) {
    let q = orders.entry(owner).or_default();
    match q.iter_mut().find(|(t, _)| *t == tag) {
        Some((_, k)) => *k &= keep,
        None => q.push((tag, keep)),
    }
}

fn ready(g: &Inner, d: &Deferral) -> bool {
    match *d {
        Deferral::Orders { owner, .. } => g.orders.get(&owner).is_some_and(|v| !v.is_empty()),
        Deferral::Breaks {
            key,
            owner,
            trigger,
            ..
        } => g.files.get(&key).is_none_or(|list| {
            !list.iter().any(|h| {
                h.owner != owner
                    && h.breaking.is_some()
                    && break_to(trigger, h.state).is_some_and(|(_, wait)| wait)
            })
        }),
    }
}
