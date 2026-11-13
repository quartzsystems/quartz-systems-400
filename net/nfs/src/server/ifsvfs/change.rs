// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Making, removing, renaming and linking. An exclusive create keeps
//! its verifier in the new file's times (access time the first four
//! bytes as seconds, modification time the last four), as RFC 1813
//! suggests, so a retransmission after the duplicate request cache has
//! forgotten the call still finds its file; the client sets the real
//! times next. With `NOSUID` set-user-ID and set-group-ID bits are
//! cleared on everything made. A file made for a host with a data
//! CCSID is tagged with it.

use ifs::Errno;
use ifs::client::IfsClient;
use ifs::ops::flags::{O_CCSID, O_CREAT, O_EXCL, O_WRONLY};
use ifs::ops::{OpenArgs, RenameMode, Stat};

use super::attr::{attr_of, ftype_of, is_dir, wcc_of};
use super::{As, Fail, Fr, IfsVfs, join, node_of};
use crate::server::error::{VResult, VfsError};
use crate::server::handle::FileHandle;
use crate::server::text;
use crate::server::vfs::{CreateHow, FileType, Made, Req, SetAttr, Special, Time, Wcc};

/// The verifier as the two times it is kept in.
fn verf_times(v: [u8; 8]) -> (i64, i64) {
    let a = u32::from_be_bytes([v[0], v[1], v[2], v[3]]);
    let m = u32::from_be_bytes([v[4], v[5], v[6], v[7]]);
    (i64::from(a) * 1_000_000_000, i64::from(m) * 1_000_000_000)
}

fn verf_matches(st: &Stat, v: [u8; 8]) -> bool {
    let (a, m) = verf_times(v);
    Time::from_nanos(st.atime).secs == Time::from_nanos(a).secs
        && Time::from_nanos(st.mtime).secs == Time::from_nanos(m).secs
}

/// A name to make or remove: not `.` or `..`.
fn new_name(r: &Req, name: &[u8]) -> Result<String, VfsError> {
    match name {
        b"." | b".." => Err(VfsError::Exist),
        _ => text::name(r.opts.path_ccsid, name),
    }
}

impl<C: IfsClient> IfsVfs<C> {
    /// The directory's path and attributes; it must be one.
    fn dir_of(&self, a: &As<'_>, r: &Req, dir: &FileHandle) -> Fr<(String, Stat)> {
        let (p, st) = self.locate(a, r, dir)?;
        if !is_dir(&st) {
            return Err(Fail::V(VfsError::NotDir));
        }
        Ok((p, st))
    }

    /// What a call that made `path` in `dir` answers.
    fn made(r: &Req, a: &As<'_>, dpath: &str, dst: &Stat, path: &str) -> Made {
        let node = a.c.stat(path, false).ok().map(|s| node_of(r, &s));
        let after = a.c.stat(dpath, false).ok().map(|s| attr_of(r, &s));
        Made {
            node,
            dir: Wcc {
                before: Some(wcc_of(dst)),
                after,
            },
        }
    }

    fn mode_for(r: &Req, s: &SetAttr, dflt: u32) -> u32 {
        let m = s.mode.unwrap_or(dflt) & 0o7777;
        if r.export.nosuid() { m & !0o6000 } else { m }
    }

    pub(crate) fn do_create(&self, r: &Req, dir: &FileHandle, name: &[u8], how: CreateHow) -> VResult<Made> {
        let n = new_name(r, name)?;
        self.run(r, |a| {
            let (dpath, dst) = self.dir_of(a, r, dir)?;
            let p = join(&dpath, &n);
            let (excl, sattr) = match how {
                CreateHow::Unchecked(s) => (false, Some(s)),
                CreateHow::Guarded(s) => (true, Some(s)),
                CreateHow::Exclusive(_) => (true, None),
            };
            let existed = if excl { None } else { a.c.stat(&p, false).ok() };
            let mode = sattr.map_or(0o600, |s| Self::mode_for(r, &s, 0o644));
            let mut args = OpenArgs::new(O_WRONLY | O_CREAT | if excl { O_EXCL } else { 0 }).mode(mode);
            if let Some(c) = r.opts.data_ccsid {
                args.oflag |= O_CCSID;
                args.conversion_id = u32::from(c);
            }
            match a.c.open(&p, args) {
                Ok(fd) => {
                    let _ = a.c.close(fd);
                }
                Err(Errno::EEXIST) => {
                    if let CreateHow::Exclusive(v) = how {
                        let st = a.c.stat(&p, false)?;
                        if ftype_of(st.mode) == FileType::Reg && verf_matches(&st, v) {
                            return Ok(Self::made(r, a, &dpath, &dst, &p));
                        }
                    }
                    return Err(Fail::V(VfsError::Exist));
                }
                Err(e) => return Err(e.into()),
            }
            match (how, existed) {
                (CreateHow::Exclusive(v), _) => {
                    a.c.utime(&p, Some(verf_times(v)))?;
                }
                (_, Some(old)) if ftype_of(old.mode) == FileType::Dir => {
                    return Err(Fail::V(VfsError::IsDir));
                }
                (_, old) => {
                    let st = a.c.stat(&p, false)?;
                    let mut s = sattr.unwrap_or_default();
                    if old.is_none() {
                        s.mode = None;
                    }
                    let fh = super::handle_of(r, &st);
                    self.apply(a, r, &fh, &p, &st, &s)?;
                }
            }
            Ok(Self::made(r, a, &dpath, &dst, &p))
        })
    }

    pub(crate) fn do_mkdir(&self, r: &Req, dir: &FileHandle, name: &[u8], s: &SetAttr) -> VResult<Made> {
        let n = new_name(r, name)?;
        self.run(r, |a| {
            let (dpath, dst) = self.dir_of(a, r, dir)?;
            let p = join(&dpath, &n);
            a.c.mkdir(&p, Self::mode_for(r, s, 0o755))?;
            self.after_make(a, r, &p, s)?;
            Ok(Self::made(r, a, &dpath, &dst, &p))
        })
    }

    /// Owner, group and times of something just made.
    fn after_make(&self, a: &As<'_>, r: &Req, p: &str, s: &SetAttr) -> Fr<()> {
        let rest = SetAttr {
            mode: None,
            size: None,
            ..*s
        };
        if rest != SetAttr::default() {
            let st = a.c.stat(p, false)?;
            let fh = super::handle_of(r, &st);
            self.apply(a, r, &fh, p, &st, &rest)?;
        }
        Ok(())
    }

    pub(crate) fn do_symlink(&self, r: &Req, dir: &FileHandle, name: &[u8], target: &[u8], s: &SetAttr) -> VResult<Made> {
        let n = new_name(r, name)?;
        let t = text::from_client(r.opts.path_ccsid, target)?;
        if t.is_empty() || t.contains('\0') {
            return Err(VfsError::Inval);
        }
        self.run(r, |a| {
            let (dpath, dst) = self.dir_of(a, r, dir)?;
            let p = join(&dpath, &n);
            a.c.symlink(&t, &p)?;
            self.after_make(a, r, &p, s)?;
            Ok(Self::made(r, a, &dpath, &dst, &p))
        })
    }

    pub(crate) fn do_mknod(&self, r: &Req, dir: &FileHandle, name: &[u8], what: Special, s: &SetAttr) -> VResult<Made> {
        if what != Special::Fifo {
            return Err(VfsError::NotSupp);
        }
        let n = new_name(r, name)?;
        self.run(r, |a| {
            let (dpath, dst) = self.dir_of(a, r, dir)?;
            let p = join(&dpath, &n);
            a.c.mkfifo(&p, Self::mode_for(r, s, 0o644))?;
            self.after_make(a, r, &p, s)?;
            Ok(Self::made(r, a, &dpath, &dst, &p))
        })
    }

    pub(crate) fn do_remove(&self, r: &Req, dir: &FileHandle, name: &[u8], rmdir: bool) -> VResult<Wcc> {
        let n = match name {
            b"." => return Err(VfsError::Inval),
            b".." => return Err(if rmdir { VfsError::Exist } else { VfsError::Inval }),
            _ => text::name(r.opts.path_ccsid, name)?,
        };
        self.run(r, |a| {
            let (dpath, dst) = self.dir_of(a, r, dir)?;
            let p = join(&dpath, &n);
            let st = a.c.stat(&p, false)?;
            let fh = super::handle_of(r, &st);
            match (rmdir, is_dir(&st)) {
                (false, true) => return Err(Fail::V(VfsError::IsDir)),
                (true, false) => return Err(Fail::V(VfsError::NotDir)),
                _ => {}
            }
            self.forget_object(fh.fid);
            if rmdir {
                a.c.rmdir(&p)?;
            } else {
                a.c.unlink(&p)?;
            }
            let after = a.c.stat(&dpath, false).ok().map(|s| attr_of(r, &s));
            Ok(Wcc {
                before: Some(wcc_of(&dst)),
                after,
            })
        })
    }

    pub(crate) fn do_rename(&self, r: &Req, from: &FileHandle, fname: &[u8], to: &FileHandle, tname: &[u8]) -> VResult<(Wcc, Wcc)> {
        let dots = |n: &[u8]| n == b"." || n == b"..";
        if dots(fname) || dots(tname) {
            return Err(VfsError::Inval);
        }
        let fname = text::name(r.opts.path_ccsid, fname)?;
        let tname = text::name(r.opts.path_ccsid, tname)?;
        self.run(r, |a| {
            let (fdir, fst) = self.dir_of(a, r, from)?;
            let (tdir, tst) = self.dir_of(a, r, to)?;
            let old = join(&fdir, &fname);
            let new = join(&tdir, &tname);
            let src = a.c.stat(&old, false)?;
            self.forget_object(super::handle_of(r, &src).fid);
            if let Ok(t) = a.c.stat(&new, false) {
                self.forget_object(super::handle_of(r, &t).fid);
            }
            a.c.rename(&old, &new, RenameMode::Unlink)?;
            let fa = a.c.stat(&fdir, false).ok().map(|s| attr_of(r, &s));
            let ta = a.c.stat(&tdir, false).ok().map(|s| attr_of(r, &s));
            Ok((
                Wcc {
                    before: Some(wcc_of(&fst)),
                    after: fa,
                },
                Wcc {
                    before: Some(wcc_of(&tst)),
                    after: ta,
                },
            ))
        })
    }

    pub(crate) fn do_link(&self, r: &Req, fh: &FileHandle, dir: &FileHandle, name: &[u8]) -> VResult<(Option<crate::server::vfs::Attr>, Wcc)> {
        let n = new_name(r, name)?;
        self.run(r, |a| {
            let (fpath, fst) = self.locate(a, r, fh)?;
            if is_dir(&fst) {
                return Err(Fail::V(VfsError::IsDir));
            }
            let (dpath, dst) = self.dir_of(a, r, dir)?;
            let p = join(&dpath, &n);
            a.c.link(&fpath, &p)?;
            let fa = a.c.stat(&fpath, false).ok().map(|s| attr_of(r, &s));
            let da = a.c.stat(&dpath, false).ok().map(|s| attr_of(r, &s));
            Ok((
                fa,
                Wcc {
                    before: Some(wcc_of(&dst)),
                    after: da,
                },
            ))
        })
    }
}
