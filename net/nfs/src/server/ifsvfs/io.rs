// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Finding objects, their attributes, reading and writing.

use std::time::{SystemTime, UNIX_EPOCH};

use ifs::client::{CHUNK, IfsClient};
use ifs::ops::Stat;
use ifs::ops::flags::{PC_LINK_MAX, R_OK, W_OK, X_OK};

use super::attr::{attr_of, ftype_of, is_dir, wcc_of};
use super::fdcache::Mode;
use super::{As, Fail, Fr, IfsVfs, join, node_of, parent};
use crate::server::error::{VResult, VfsError};
use crate::server::export::within;
use crate::server::gate::writable;
use crate::server::handle::FileHandle;
use crate::server::text::{self, Xlate};
use crate::server::vfs::{
    Attr, FileType, FsStat, Node, PathConf, ReadData, Req, SetAttr, SetTime, Stable, Time, Wcc,
    Written,
};
use crate::types::nfs3::{
    ACCESS3_DELETE, ACCESS3_EXECUTE, ACCESS3_EXTEND, ACCESS3_LOOKUP, ACCESS3_MODIFY, ACCESS3_READ,
};

fn now_nanos() -> i64 {
    let d = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
    i64::try_from(d.as_nanos()).unwrap_or(i64::MAX)
}

impl<C: IfsClient> IfsVfs<C> {
    pub(crate) fn do_lookup_path(&self, r: &Req, path: &[u8]) -> VResult<Node> {
        let p = text::from_client(r.opts.path_ccsid, path)?;
        if p.contains('\0') || !p.starts_with('/') {
            return Err(VfsError::Inval);
        }
        if within(r.export.path(), &p).is_none() {
            return Err(VfsError::Acces);
        }
        self.run(r, |a| {
            let st = a.c.stat(&p, true)?;
            if !is_dir(&st) {
                return Err(Fail::V(VfsError::NotDir));
            }
            Ok(node_of(r, &st))
        })
    }

    pub(crate) fn do_lookup(&self, r: &Req, dir: &FileHandle, name: &[u8]) -> VResult<(Node, Option<Attr>)> {
        self.run(r, |a| {
            let (path, st) = self.locate(a, r, dir)?;
            if !is_dir(&st) {
                return Err(Fail::V(VfsError::NotDir));
            }
            let dattr = attr_of(r, &st);
            let target = match name {
                b"." => st.clone(),
                b".." if Self::is_root(r, dir) => st.clone(),
                b".." => a.c.stat(parent(&path), true)?,
                _ => {
                    let n = text::name(r.opts.path_ccsid, name)?;
                    // Search authority on the directory, as a walk needs.
                    a.c.stat(&join(&path, &n), false)?
                }
            };
            Ok((node_of(r, &target), Some(dattr)))
        })
    }

    pub(crate) fn do_access(&self, r: &Req, fh: &FileHandle, want: u32) -> VResult<(u32, Option<Attr>)> {
        let w = writable(r);
        self.run(r, |a| {
            let (path, st) = self.locate(a, r, fh)?;
            let dir = is_dir(&st);
            let ok = |m: i32| a.c.access(&path, m).is_ok();
            let mut got = 0;
            if want & ACCESS3_READ != 0 && ok(R_OK) {
                got |= ACCESS3_READ;
            }
            let x = if dir { ACCESS3_LOOKUP } else { ACCESS3_EXECUTE };
            if want & x != 0 && ok(X_OK) {
                got |= x;
            }
            let mut wbits = ACCESS3_MODIFY | ACCESS3_EXTEND;
            if dir {
                wbits |= ACCESS3_DELETE;
            }
            if w && want & wbits != 0 && ok(W_OK) {
                got |= want & wbits;
            }
            Ok((got, Some(attr_of(r, &st))))
        })
    }

    pub(crate) fn do_readlink(&self, r: &Req, fh: &FileHandle) -> VResult<(Vec<u8>, Option<Attr>)> {
        self.run(r, |a| {
            let (path, st) = self.locate(a, r, fh)?;
            if ftype_of(st.mode) != FileType::Lnk {
                return Err(Fail::V(VfsError::Inval));
            }
            let t = a.c.readlink(&path)?;
            Ok((text::to_client(r.opts.path_ccsid, &t)?, Some(attr_of(r, &st))))
        })
    }

    /// A descriptor for I/O on the object; one that fails is opened again once.
    fn io<T>(&self, a: &As<'_>, st: &Stat, fh: &FileHandle, mode: Mode, f: impl Fn(i32) -> Result<T, ifs::Errno>) -> Fr<T> {
        let _ = st;
        let fd = self.fd(a, fh.fid, mode)?;
        match f(fd) {
            Err(ifs::Errno::EBADF) => {
                self.drop_fd(fd);
                let fd = self.fd(a, fh.fid, mode)?;
                Ok(f(fd)?)
            }
            r => Ok(r?),
        }
    }

    pub(crate) fn do_read(&self, r: &Req, fh: &FileHandle, off: u64, count: u32) -> VResult<ReadData> {
        self.run(r, |a| {
            let (path, st) = self.locate(a, r, fh)?;
            match ftype_of(st.mode) {
                FileType::Dir => return Err(Fail::V(VfsError::IsDir)),
                FileType::Reg => {}
                _ => return Err(Fail::V(VfsError::Inval)),
            }
            a.c.access(&path, R_OK)?;
            let want = usize::try_from(count).unwrap_or(CHUNK).min(CHUNK);
            let mut data = if want == 0 || off >= st.size {
                Vec::new()
            } else {
                self.io(a, &st, fh, Mode::Read, |fd| a.c.pread(fd, want, off))?
            };
            let end = off.saturating_add(data.len() as u64);
            let eof = end >= st.size;
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

    pub(crate) fn do_write(&self, r: &Req, fh: &FileHandle, off: u64, data: &[u8], stable: Stable) -> VResult<Written> {
        self.run(r, |a| {
            let (path, st) = self.locate(a, r, fh)?;
            match ftype_of(st.mode) {
                FileType::Dir => return Err(Fail::V(VfsError::IsDir)),
                FileType::Reg => {}
                _ => return Err(Fail::V(VfsError::Inval)),
            }
            a.c.access(&path, W_OK)?;
            let mut buf = data.to_vec();
            if let Some(x) = Xlate::between(st.ccsid, r.opts.data_ccsid) {
                x.to_file(&mut buf);
            }
            let mut done = 0usize;
            while done < buf.len() {
                let at = off.saturating_add(done as u64);
                let n = self.io(a, &st, fh, Mode::Write, |fd| {
                    a.c.pwrite(fd, &buf[done..(done + CHUNK).min(buf.len())], at)
                })?;
                if n == 0 {
                    return Err(Fail::V(VfsError::NoSpc));
                }
                done += n;
            }
            let committed = if r.opts.sync { Stable::FileSync } else { stable };
            let fd = self.fd(a, fh.fid, Mode::Write)?;
            if committed != Stable::Unstable {
                a.c.fsync(fd, committed == Stable::DataSync)?;
            }
            let after = a.c.fstat(fd).ok().map(|s| attr_of(r, &s));
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

    pub(crate) fn do_commit(&self, r: &Req, fh: &FileHandle, _off: u64, _count: u32) -> VResult<([u8; 8], Wcc)> {
        self.run(r, |a| {
            let (_, st) = self.locate(a, r, fh)?;
            if is_dir(&st) {
                return Err(Fail::V(VfsError::IsDir));
            }
            let fd = match self.fd(a, fh.fid, Mode::Write) {
                Ok(fd) => fd,
                Err(_) => self.fd(a, fh.fid, Mode::Read)?,
            };
            a.c.fsync(fd, false)?;
            let after = a.c.fstat(fd).ok().map(|s| attr_of(r, &s));
            Ok((
                self.verf(),
                Wcc {
                    before: Some(wcc_of(&st)),
                    after,
                },
            ))
        })
    }

    /// Applies `s` to the object at `path` (now `st`).
    pub(crate) fn apply(&self, a: &As<'_>, r: &Req, fh: &FileHandle, path: &str, st: &Stat, s: &SetAttr) -> Fr<()> {
        let link = ftype_of(st.mode) == FileType::Lnk;
        if let Some(m) = s.mode {
            let mut m = m & 0o7777;
            if r.export.nosuid() {
                m &= !0o6000;
            }
            if !link {
                a.c.chmod(path, m)?;
            }
        }
        if s.uid.is_some() || s.gid.is_some() {
            let uid = s.uid.unwrap_or(st.uid);
            let gid = s.gid.unwrap_or(st.gid);
            if uid != st.uid || gid != st.gid {
                if link {
                    a.c.lchown(path, uid, gid)?;
                } else {
                    a.c.chown(path, uid, gid)?;
                }
            }
        }
        if let Some(size) = s.size {
            match ftype_of(st.mode) {
                FileType::Dir => return Err(Fail::V(VfsError::IsDir)),
                FileType::Reg => {}
                _ => return Err(Fail::V(VfsError::Inval)),
            }
            if size != st.size {
                a.c.access(path, W_OK)?;
                self.io(a, st, fh, Mode::Write, |fd| a.c.ftruncate(fd, size))?;
            }
        }
        if s.atime != SetTime::Keep || s.mtime != SetTime::Keep {
            let pick = |t: SetTime, cur: i64| match t {
                SetTime::Keep => cur,
                SetTime::Server => now_nanos(),
                SetTime::Client(t) => t.nanos(),
            };
            if s.atime == SetTime::Server && s.mtime == SetTime::Server {
                a.c.utime(path, None)?;
            } else {
                a.c.utime(path, Some((pick(s.atime, st.atime), pick(s.mtime, st.mtime))))?;
            }
        }
        Ok(())
    }

    pub(crate) fn do_setattr(&self, r: &Req, fh: &FileHandle, s: &SetAttr, guard: Option<Time>) -> VResult<Wcc> {
        self.run(r, |a| {
            let (path, st) = self.locate(a, r, fh)?;
            if let Some(g) = guard
                && g != Time::from_nanos(st.ctime)
            {
                return Err(Fail::V(VfsError::NotSync));
            }
            self.apply(a, r, fh, &path, &st, s)?;
            let after = a.c.stat(&path, false).ok().map(|s| attr_of(r, &s));
            Ok(Wcc {
                before: Some(wcc_of(&st)),
                after,
            })
        })
    }

    pub(crate) fn do_fsstat(&self, r: &Req, fh: &FileHandle) -> VResult<(FsStat, Option<Attr>)> {
        self.run(r, |a| {
            let (path, st) = self.locate(a, r, fh)?;
            let v = a.c.statvfs(&path)?;
            let unit = if v.frsize == 0 { v.bsize } else { v.frsize };
            Ok((
                FsStat {
                    tbytes: v.blocks.saturating_mul(unit),
                    fbytes: v.bfree.saturating_mul(unit),
                    abytes: v.bavail.saturating_mul(unit),
                    tfiles: v.files,
                    ffiles: v.ffree,
                    afiles: v.favail,
                },
                Some(attr_of(r, &st)),
            ))
        })
    }

    pub(crate) fn do_pathconf(&self, r: &Req, fh: &FileHandle) -> VResult<(PathConf, Option<Attr>)> {
        self.run(r, |a| {
            let (path, st) = self.locate(a, r, fh)?;
            let v = a.c.statvfs(&path)?;
            let linkmax = a
                .c
                .pathconf(&path, PC_LINK_MAX)
                .ok()
                .flatten()
                .and_then(|n| u32::try_from(n).ok())
                .unwrap_or(u32::MAX);
            Ok((
                PathConf {
                    linkmax,
                    name_max: u32::try_from(v.namemax).unwrap_or(255).min(255),
                    no_trunc: true,
                    chown_restricted: true,
                    case_insensitive: !v.case_sensitive,
                    case_preserving: true,
                },
                Some(attr_of(r, &st)),
            ))
        })
    }
}
