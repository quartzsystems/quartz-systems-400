// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A lease holder's calls ([`crate::lease`]): reserve a lease on a file
//! the caller can reach, report the leases it holds, poll for break
//! orders.

use std::time::Duration;

use super::audit::FileId;
use super::{Ctx, Ifs};
use crate::auth::need::Op;
use crate::backend::Backend;
use crate::errno::{Errno, Result};
use crate::lease::{LeaseKey, News, Others};
use crate::lock::Held;
use crate::resolve::Node;
use crate::vfs::qsys::QsysNode;

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
        // A member open for change by a database job counts as open.
        if let Node::Qsys(QsysNode::Mbr(l, f, m), _) = &r.node {
            others.open = sys.qsys.member_busy(l, f, m);
        }
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

    /// The caller's break orders and its watches' changes, waiting up to
    /// `wait` for some when its caller allows a deferral.
    ///
    /// # Errors
    ///
    /// `EAGAIN` with the deferral.
    pub fn lease_orders(&self, cx: &Ctx<'_>, wait: Duration) -> Result<News> {
        self.leases.orders(cx.owner, wait)
    }

    /// Watch the directory at `path` (and below it when `tree`) for the
    /// caller ([`crate::lease::notify`]): the watch's number. The caller
    /// needs to be able to list it.
    ///
    /// # Errors
    ///
    /// The path's resolution, `ENOTDIR`, the authority to list it.
    pub fn watch_add(&self, cx: &Ctx<'_>, path: &str, tree: bool) -> Result<u64> {
        let mut st = self.lock();
        let cwd = Self::cwd_of(&mut st, cx.owner);
        let (mut sys, _) = self.split(&mut st);
        let r = sys.resolve(&cwd, path, true, Some(cx))?;
        if !r.node.is_dir() {
            return Err(Errno::ENOTDIR);
        }
        let (m, _) = sys.meta(&r.node, Some(cx))?;
        cx.require(&m, &r.node, &r.path(), Op::Readdir)?;
        Ok(self.leases.watch_add(cx.owner, r.path(), tree))
    }

    /// The caller's server of leases and watches ended: they go, and its
    /// waiting poll answers at once ([`crate::lease::Leases::reset_owner`]).
    pub fn lease_reset(&self, cx: &Ctx<'_>) {
        self.leases.reset_owner(cx.owner);
    }

    /// End the caller's watch `id`.
    pub fn watch_end(&self, cx: &Ctx<'_>, id: u64) {
        self.leases.watch_end(cx.owner, id);
    }
}
