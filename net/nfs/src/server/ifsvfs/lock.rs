// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Opens with share reservations and byte-range locks of sub-owners.
//! `NFSv4` and NLM `SHARE` access and deny map to the IFS's open flags
//! and share modes (docs/IFS.md "Share reservations"). Locks are held
//! through a kept descriptor (read locks on one opened for reading,
//! write locks on one opened for writing); a sub-owner's locks outlive
//! the descriptor. Offsets past `i64::MAX` are clamped, and a range
//! that would end past it runs to the end of file and beyond.

use ifs::Errno;
use ifs::client::{Caller, IfsClient};
use ifs::lock::range::{RangeKind, RangeLock};
use ifs::ops::OpenArgs;
use ifs::ops::flags::{
    F_GETLK, F_SETLK, O_RDONLY, O_RDWR, O_SHARE_NONE, O_SHARE_RDONLY, O_SHARE_RDWR, O_SHARE_WRONLY,
    O_WRONLY, SEEK_SET,
};

use super::fdcache::Mode;
use super::{Fail, IfsVfs};
use crate::server::error::VResult;
use crate::server::handle::FileHandle;
use crate::server::vfs::{Conflict, LockKind, LockOutcome, Req, ShareAccess, ShareDeny};

/// The IFS's start and length of a range.
pub(super) fn range(start: u64, len: u64) -> (i64, i64) {
    let s = i64::try_from(start).unwrap_or(i64::MAX);
    let l = match i64::try_from(len) {
        Ok(l) if s.checked_add(l).is_some() => l,
        _ => 0,
    };
    (s, l)
}

pub(super) fn conflict(l: &RangeLock) -> Conflict {
    Conflict {
        owner: l.owner.0,
        sub: l.sub,
        kind: if l.kind == RangeKind::Write {
            LockKind::Write
        } else {
            LockKind::Read
        },
        start: l.start,
        len: if l.end == u64::MAX {
            0
        } else {
            l.end - l.start
        },
    }
}

fn kind_of(k: LockKind) -> (RangeKind, Mode) {
    match k {
        LockKind::Read => (RangeKind::Read, Mode::Read),
        LockKind::Write => (RangeKind::Write, Mode::Write),
    }
}

/// Whom the descriptors that hold locks are kept for: the job.
static JOB: Caller = Caller::Profile {
    name: String::new(),
};

impl<C: IfsClient> IfsVfs<C> {
    /// A descriptor that holds byte-range locks: the job's own. A lock is
    /// advisory, so the caller needs only to reach the object (checked by
    /// `locate` as the caller), as a Unix lock manager asks.
    fn lock_fd(&self, fh: &FileHandle, mode: Mode) -> Result<i32, Fail> {
        let job = super::As {
            c: self.client(),
            who: &JOB,
        };
        self.fd(&job, fh.fid, mode)
    }

    pub(crate) fn do_open(
        &self,
        r: &Req,
        fh: &FileHandle,
        access: ShareAccess,
        deny: ShareDeny,
    ) -> VResult<u64> {
        let acc = match access {
            ShareAccess::Read => O_RDONLY,
            ShareAccess::Write => O_WRONLY,
            ShareAccess::Both => O_RDWR,
        };
        let share = match deny {
            ShareDeny::None => O_SHARE_RDWR,
            ShareDeny::Read => O_SHARE_WRONLY,
            ShareDeny::Write => O_SHARE_RDONLY,
            ShareDeny::Both => O_SHARE_NONE,
        };
        let fd = self.run(r, |a| {
            let (_, _st) = self.locate(a, r, fh)?;
            match a.c.open_by_id(fh.fid, OpenArgs::new(acc | share)) {
                Ok((fd, _)) => Ok(fd),
                Err(Errno::EBUSY) => {
                    // Kept descriptors, this thread's or another idle
                    // thread's of the job, may be what is in the way.
                    self.forget_object_everywhere(fh.fid);
                    Ok(a.c.open_by_id(fh.fid, OpenArgs::new(acc | share))?.0)
                }
                Err(e) => Err(Fail::from(e)),
            }
        })?;
        Ok(Self::new_open(fd))
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn do_lock(
        &self,
        r: &Req,
        fh: &FileHandle,
        sub: u64,
        kind: LockKind,
        start: u64,
        len: u64,
        wait: bool,
    ) -> VResult<LockOutcome> {
        let (rk, mode) = kind_of(kind);
        let (s, l) = range(start, len);
        self.run(r, |a| {
            let _ = self.locate(a, r, fh)?;
            let fd = self.lock_fd(fh, mode)?;
            if wait {
                return Ok(match a.c.lock_wait(fd, sub, rk, SEEK_SET, s, l)? {
                    None => LockOutcome::Granted,
                    Some(t) => LockOutcome::Queued(t),
                });
            }
            match a.c.lock_range_as(fd, sub, F_SETLK, rk, SEEK_SET, s, l) {
                Ok(_) => Ok(LockOutcome::Granted),
                Err(Errno::EAGAIN) => {
                    let got = a.c.lock_range_as(fd, sub, F_GETLK, rk, SEEK_SET, s, l)?;
                    match got {
                        Some(h) => Ok(LockOutcome::Denied(conflict(&h))),
                        // Released in between: try once more.
                        None => match a.c.lock_range_as(fd, sub, F_SETLK, rk, SEEK_SET, s, l) {
                            Ok(_) => Ok(LockOutcome::Granted),
                            Err(e) => Err(Fail::from(e)),
                        },
                    }
                }
                Err(e) => Err(Fail::from(e)),
            }
        })
    }

    pub(crate) fn do_test_lock(
        &self,
        r: &Req,
        fh: &FileHandle,
        sub: u64,
        kind: LockKind,
        start: u64,
        len: u64,
    ) -> VResult<Option<crate::server::vfs::Conflict>> {
        let (rk, mode) = kind_of(kind);
        let (s, l) = range(start, len);
        self.run(r, |a| {
            let _ = self.locate(a, r, fh)?;
            let fd = self.lock_fd(fh, mode)?;
            Ok(a.c
                .lock_range_as(fd, sub, F_GETLK, rk, SEEK_SET, s, l)?
                .as_ref()
                .map(conflict))
        })
    }

    pub(crate) fn do_unlock(
        &self,
        r: &Req,
        fh: &FileHandle,
        sub: u64,
        start: u64,
        len: u64,
    ) -> VResult<()> {
        let (s, l) = range(start, len);
        self.run(r, |a| {
            let _ = self.locate(a, r, fh)?;
            let fd = self.lock_fd(fh, Mode::Read)?;
            a.c.lock_range_as(fd, sub, F_SETLK, RangeKind::Unlock, SEEK_SET, s, l)?;
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::range;

    #[test]
    fn ranges_clamp() {
        assert_eq!(range(0, 0), (0, 0));
        assert_eq!(range(10, 5), (10, 5));
        assert_eq!(range(u64::MAX, 5), (i64::MAX, 0));
        assert_eq!(range(5, u64::MAX), (5, 0));
    }
}
