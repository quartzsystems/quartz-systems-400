// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! I/O and locks through an `NFSv4` open's own descriptor: share
//! reservations are enforced between every two opens, the job's own
//! too, so a kept descriptor of [`super::fdcache`] would collide with
//! the open that denies what it does. Authority was checked when the
//! open was made.

use ifs::Errno;
use ifs::client::{CHUNK, IfsClient};
use ifs::lock::range::RangeKind;
use ifs::ops::flags::{F_GETLK, F_SETLK, SEEK_SET};

use super::attr::{attr_of, ftype_of, wcc_of};
use super::{Fail, IfsVfs};
use crate::server::error::{VResult, VfsError};
use crate::server::handle::FileHandle;
use crate::server::text::Xlate;
use crate::server::vfs::{
    FileType, LockKind, LockOutcome, ReadData, Req, SetAttr, Stable, Time, Wcc, Written,
};

fn fd_of(open: u64) -> Result<i32, Fail> {
    i32::try_from(open).map_err(|_| Fail::V(VfsError::Inval))
}

fn regular(mode: u32) -> Result<(), Fail> {
    match ftype_of(mode) {
        FileType::Dir => Err(Fail::V(VfsError::IsDir)),
        FileType::Reg => Ok(()),
        _ => Err(Fail::V(VfsError::Inval)),
    }
}

impl<C: IfsClient> IfsVfs<C> {
    pub(crate) fn do_read_open(
        &self,
        r: &Req,
        fh: &FileHandle,
        open: u64,
        off: u64,
        count: u32,
    ) -> VResult<ReadData> {
        self.run(r, |a| {
            let (_, st) = self.locate(a, r, fh)?;
            regular(st.mode)?;
            let fd = fd_of(open)?;
            let want = usize::try_from(count).unwrap_or(CHUNK).min(CHUNK);
            let mut data = if want == 0 || off >= st.size {
                Vec::new()
            } else {
                a.c.pread(fd, want, off)?
            };
            let end = off.saturating_add(data.len() as u64);
            let eof = end >= st.size || data.len() < want;
            if let Some(x) = Xlate::between(st.ccsid, r.opts.data_ccsid) {
                x.to_client(&mut data);
            }
            Ok(ReadData {
                data,
                eof,
                attr: Some(attr_of(r, &st)),
            })
        })
    }

    pub(crate) fn do_write_open(
        &self,
        r: &Req,
        fh: &FileHandle,
        open: u64,
        off: u64,
        data: &[u8],
        stable: Stable,
    ) -> VResult<Written> {
        self.run(r, |a| {
            let (_, st) = self.locate(a, r, fh)?;
            regular(st.mode)?;
            let fd = fd_of(open)?;
            let mut buf = data.to_vec();
            if let Some(x) = Xlate::between(st.ccsid, r.opts.data_ccsid) {
                x.to_file(&mut buf);
            }
            let committed = if r.opts.sync {
                Stable::FileSync
            } else {
                stable
            };
            let sync = (committed != Stable::Unstable).then_some(committed == Stable::DataSync);
            let (done, after) = super::io::write_all(&buf, off, sync, |piece, at, sync| {
                Ok(a.c.pwrite_stat(fd, piece, at, sync)?)
            })?;
            let after = after.map(|s| attr_of(r, &s));
            Ok(Written {
                count: u32::try_from(done).unwrap_or(u32::MAX),
                committed,
                verf: self.verf(),
                wcc: Wcc {
                    before: Some(wcc_of(&st)),
                    after,
                },
            })
        })
    }

    pub(crate) fn do_setattr_open(
        &self,
        r: &Req,
        fh: &FileHandle,
        open: u64,
        s: &SetAttr,
    ) -> VResult<Wcc> {
        self.run(r, |a| {
            let (path, st) = self.locate(a, r, fh)?;
            if let Some(size) = s.size {
                regular(st.mode)?;
                if size != st.size {
                    a.c.ftruncate(fd_of(open)?, size)?;
                }
            }
            let rest = SetAttr { size: None, ..*s };
            self.apply(a, r, fh, &path, &st, &rest)?;
            let after = a.c.stat(&path, false).ok().map(|x| attr_of(r, &x));
            Ok(Wcc {
                before: Some(wcc_of(&st)),
                after,
            })
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn do_lock_open(
        &self,
        r: &Req,
        fh: &FileHandle,
        open: u64,
        sub: u64,
        kind: LockKind,
        start: u64,
        len: u64,
    ) -> VResult<LockOutcome> {
        let rk = match kind {
            LockKind::Read => RangeKind::Read,
            LockKind::Write => RangeKind::Write,
        };
        let (s, l) = super::lock::range(start, len);
        self.run(r, |a| {
            let _ = self.locate(a, r, fh)?;
            let fd = fd_of(open)?;
            match a.c.lock_range_as(fd, sub, F_SETLK, rk, SEEK_SET, s, l) {
                Ok(_) => Ok(LockOutcome::Granted),
                Err(Errno::EAGAIN) => {
                    match a.c.lock_range_as(fd, sub, F_GETLK, rk, SEEK_SET, s, l)? {
                        Some(h) => Ok(LockOutcome::Denied(super::lock::conflict(&h))),
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

    pub(crate) fn do_unlock_open(
        &self,
        r: &Req,
        fh: &FileHandle,
        open: u64,
        sub: u64,
        start: u64,
        len: u64,
    ) -> VResult<()> {
        let (s, l) = super::lock::range(start, len);
        self.run(r, |a| {
            let _ = self.locate(a, r, fh)?;
            let fd = fd_of(open)?;
            a.c.lock_range_as(fd, sub, F_SETLK, RangeKind::Unlock, SEEK_SET, s, l)?;
            Ok(())
        })
    }

    pub(crate) fn do_created(&self, r: &Req, fh: &FileHandle) -> VResult<Time> {
        self.run(r, |a| {
            let (_, st) = self.locate(a, r, fh)?;
            Ok(Time::from_nanos(st.crtime))
        })
    }
}
