// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Byte-range locks held by an owner's sub-owners, and requests that
//! wait in the lock table instead of in the caller ([`crate::lock::wait`]):
//! a server job holding locks for its own clients names each client by a
//! sub-owner, so their locks conflict with each other and with every
//! other job's. A queued request granted later is told to its owner by
//! its poll ([`crate::lease::News::locks`]).

use super::flags as f;
use super::{Ctx, Ifs};
use crate::backend::Backend;
use crate::errno::{Errno, Result};
use crate::lease::state::Trigger;
use crate::lock::ObjKey;
use crate::lock::range::{RangeKind, RangeLock};
use crate::lock::share::Access;

impl<B: Backend> Ifs<B> {
    /// The lock `fd`'s open asks for: object, path, the lock, the open's
    /// access.
    #[allow(clippy::too_many_arguments)]
    fn range_of(
        &self,
        cx: &Ctx<'_>,
        fd: i32,
        sub: u64,
        kind: RangeKind,
        whence: i32,
        start: i64,
        len: i64,
    ) -> Result<(ObjKey, String, RangeLock, Access)> {
        let st = self.lock();
        let rest = &st.rest;
        let open = rest
            .owners
            .get(&cx.owner)
            .and_then(|o| o.fds.get(&fd))
            .ok_or(Errno::EBADF)?
            .open;
        let o = rest.opens.get(&open).ok_or(Errno::EBADF)?;
        let base: i64 = match whence {
            f::SEEK_SET => 0,
            f::SEEK_CUR => i64::try_from(o.pos).map_err(|_| Errno::EOVERFLOW)?,
            f::SEEK_END => i64::try_from(Self::size_of(o)?).map_err(|_| Errno::EOVERFLOW)?,
            _ => return Err(Errno::EINVAL),
        };
        let mut abs = base.checked_add(start).ok_or(Errno::EINVAL)?;
        let mut len = len;
        if len < 0 {
            abs = abs.checked_add(len).ok_or(Errno::EINVAL)?;
            len = -len;
        }
        let abs = u64::try_from(abs).map_err(|_| Errno::EINVAL)?;
        let len = u64::try_from(len).map_err(|_| Errno::EINVAL)?;
        let want = RangeLock::new(cx.owner, kind, abs, len).with_sub(sub);
        Ok((o.key, o.path.clone(), want, o.access))
    }

    /// `fcntl(F_GETLK / F_SETLK / F_SETLKW)` as [`Ifs::lock_range`] does,
    /// for the caller's sub-owner `sub` (0: the job itself).
    ///
    /// # Errors
    ///
    /// As `fcntl()`: `EBADF`, `EINVAL`, `EAGAIN`, `EDEADLK`.
    #[allow(clippy::too_many_arguments)]
    pub fn lock_range_as(
        &self,
        cx: &Ctx<'_>,
        fd: i32,
        sub: u64,
        cmd: i32,
        kind: RangeKind,
        whence: i32,
        start: i64,
        len: i64,
    ) -> Result<Option<RangeLock>> {
        let (key, path, want, access) = self.range_of(cx, fd, sub, kind, whence, start, len)?;
        let mut st = self.lock();
        let rest = &mut st.rest;
        match cmd {
            f::F_GETLK | f::F_GETLK64 => Ok(rest.locks.get(key, &want)),
            f::F_SETLK | f::F_SETLK64 | f::F_SETLKW | f::F_SETLKW64 => {
                check_access(kind, access)?;
                let blocking = matches!(cmd, f::F_SETLKW | f::F_SETLKW64);
                if kind != RangeKind::Unlock {
                    self.leases.gate(cx.owner, key, Trigger::Write)?;
                }
                rest.locks.set(key, &path, want, blocking)?;
                self.leases.locks_granted(&rest.locks.drain_granted());
                Ok(None)
            }
            _ => Err(Errno::EINVAL),
        }
    }

    /// A read or write lock for sub-owner `sub` that waits in the table:
    /// `None` when granted at once, else a token the caller's poll names
    /// once granted ([`crate::lease::News::locks`]).
    ///
    /// # Errors
    ///
    /// `EBADF`, `EINVAL` (an unlock), `EDEADLK` when waiting would never end.
    #[allow(clippy::too_many_arguments)]
    pub fn lock_wait(
        &self,
        cx: &Ctx<'_>,
        fd: i32,
        sub: u64,
        kind: RangeKind,
        whence: i32,
        start: i64,
        len: i64,
    ) -> Result<Option<u64>> {
        let (key, path, want, access) = self.range_of(cx, fd, sub, kind, whence, start, len)?;
        check_access(kind, access)?;
        self.leases.gate(cx.owner, key, Trigger::Write)?;
        let mut st = self.lock();
        let r = st.rest.locks.enqueue(key, &path, want);
        self.leases.locks_granted(&st.rest.locks.drain_granted());
        r
    }

    /// Take back the caller's queued request `token`: whether it was
    /// still queued (a granted one is held and must be unlocked).
    pub fn lock_cancel(&self, cx: &Ctx<'_>, token: u64) -> bool {
        let mut st = self.lock();
        let r = st.rest.locks.cancel(cx.owner, token);
        self.leases.locks_granted(&st.rest.locks.drain_granted());
        r
    }

    /// Drop every lock and queued request of the caller's sub-owner
    /// `sub` (a client gone): how many locks went.
    ///
    /// # Errors
    ///
    /// `EINVAL` for sub-owner 0 (the job's own locks end with its
    /// descriptors).
    pub fn lock_release_sub(&self, cx: &Ctx<'_>, sub: u64) -> Result<u32> {
        if sub == 0 {
            return Err(Errno::EINVAL);
        }
        let mut st = self.lock();
        let n = st.rest.locks.release_sub(cx.owner, sub);
        self.leases.locks_granted(&st.rest.locks.drain_granted());
        Ok(u32::try_from(n).unwrap_or(u32::MAX))
    }
}

fn check_access(kind: RangeKind, access: Access) -> Result<()> {
    match kind {
        RangeKind::Read if !access.read => Err(Errno::EBADF),
        RangeKind::Write if !access.write => Err(Errno::EBADF),
        _ => Ok(()),
    }
}
