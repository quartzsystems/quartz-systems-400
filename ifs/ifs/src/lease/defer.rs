// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Operations that have to wait for another owner. The IFS never waits
//! under its lock: an operation that would wait for a lease break (or a
//! poll for break orders that has none yet) gives up with `EAGAIN`
//! before it changes anything and leaves a [`Deferral`] here, when its
//! caller allowed one ([`deferring`]). The caller waits for it
//! ([`super::Leases::wait`], or the storage service parks the request)
//! and runs the operation again. Without that permission the breaks are
//! sent and the operation goes on at once.

use std::cell::{Cell, RefCell};
use std::time::Instant;

use super::state::Trigger;
use crate::lock::{ObjKey, Owner};

/// What a deferred operation waits for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Deferral {
    /// Other owners' acknowledgments of breaks on `key` that `trigger`
    /// made for `owner`, until `until` at the latest.
    Breaks {
        /// The object.
        key: ObjKey,
        /// Who waits.
        owner: Owner,
        /// What it is doing.
        trigger: Trigger,
        /// When the breaks have timed out.
        until: Instant,
    },
    /// Break orders for `owner`, until `until`.
    Orders {
        /// Who polls.
        owner: Owner,
        /// When the poll ends empty.
        until: Instant,
    },
}

impl Deferral {
    /// When the wait ends whatever happens.
    #[must_use]
    pub fn until(&self) -> Instant {
        match *self {
            Self::Breaks { until, .. } | Self::Orders { until, .. } => until,
        }
    }
}

thread_local! {
    static ALLOWED: Cell<bool> = const { Cell::new(false) };
    static TAKEN: RefCell<Option<Deferral>> = const { RefCell::new(None) };
}

/// Run `f`, allowing an operation in it to defer when `allow`; what it
/// deferred, if anything (its result is then `EAGAIN` and meaningless).
pub fn deferring<T>(allow: bool, f: impl FnOnce() -> T) -> (T, Option<Deferral>) {
    let before = ALLOWED.with(|a| a.replace(allow));
    let saved = TAKEN.with(|t| t.borrow_mut().take());
    let r = f();
    ALLOWED.with(|a| a.set(before));
    let d = TAKEN.with(|t| std::mem::replace(&mut *t.borrow_mut(), saved));
    (r, d)
}

/// Run `f` with deferral off: an operation made of others (copy, move,
/// remove tree) would be left half done by a part that defers.
pub fn whole<T>(f: impl FnOnce() -> T) -> T {
    let before = ALLOWED.with(|a| a.replace(false));
    let r = f();
    ALLOWED.with(|a| a.set(before));
    r
}

/// Leave `d` for the caller, when it allowed one.
pub(super) fn defer(d: Deferral) -> bool {
    if !ALLOWED.with(Cell::get) {
        return false;
    }
    TAKEN.with(|t| *t.borrow_mut() = Some(d));
    true
}
