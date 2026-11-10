// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A lease holder's calls ([`crate::lease`]): reserve a lease on a file
//! the caller can reach, report the leases it holds, poll for break
//! orders.

use std::time::Duration;

use super::audit::FileId;
use super::{Ctx, Ifs};
use crate::backend::Backend;
use crate::errno::Result;
use crate::lease::{LeaseKey, Order, Others};
use crate::lock::Held;

impl<B: Backend> Ifs<B> {
    /// Reserve the lease `key` in state `want` on the file at `path`,
    /// which the caller knows as `id` and calls `tag`: the state other
    /// owners allow (none when `path` is no longer that file).
    ///
    /// # Errors
    ///
    /// The path's resolution.
    pub fn lease_grant(
        &self,
        cx: &Ctx<'_>,
        path: &str,
        id: FileId,
        tag: u64,
        key: LeaseKey,
        want: u8,
    ) -> Result<u8> {
        let mut st = self.lock();
        let cwd = Self::cwd_of(&mut st, cx.owner);
        let (mut sys, rest) = self.split(&mut st);
        let r = sys.resolve(&cwd, path, true, Some(cx))?;
        if r.node.is_dir() {
            return Ok(0);
        }
        let (m, _) = sys.meta(&r.node, Some(cx))?;
        let found = FileId {
            fs: r.node.fs(),
            ino: r.node.ino(),
            generation: m.generation,
        };
        if found != id {
            return Ok(0);
        }
        let obj = r.node.key();
        let mut others = Others::default();
        for l in rest.locks.list(obj) {
            if l.owner == cx.owner {
                continue;
            }
            match l.what {
                Held::Open(..) => others.open = true,
                Held::Range(_) => others.locked = true,
            }
        }
        Ok(self.leases.grant(cx.owner, obj, tag, key, want, others))
    }

    /// The caller holds `leases` (and no others) on the file it calls
    /// `tag`.
    pub fn lease_report(&self, cx: &Ctx<'_>, tag: u64, leases: &[(LeaseKey, u8)]) {
        self.leases.report(cx.owner, tag, leases);
    }

    /// The caller's break orders, waiting up to `wait` for one when its
    /// caller allows a deferral.
    ///
    /// # Errors
    ///
    /// `EAGAIN` with the deferral.
    pub fn lease_orders(&self, cx: &Ctx<'_>, wait: Duration) -> Result<Vec<Order>> {
        self.leases.orders(cx.owner, wait)
    }
}
